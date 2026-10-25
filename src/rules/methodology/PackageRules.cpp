// Methodology guide chapter MG-1, layering and packages: SV-PKG-001 to SV-PKG-005.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

#include <algorithm>
#include <map>
#include <set>
#include <unordered_set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;
using namespace slang::syntax;
using slang::parsing::Token;
using slang::parsing::TokenKind;

namespace {

/// The node that owns a syntax node (member lists are not nodes of their own).
const SyntaxNode* owner(const SyntaxNode& node) { return node.parent; }

bool userFileLoc(const slang::SourceManager& sm, slang::SourceLocation loc) {
    return loc.valid() && sm.isFileLoc(loc) && !meth::isLibraryBuffer(sm, loc.buffer());
}

template<typename F>
void forEachUserTree(Compilation& comp, F&& fn) {
    const auto* sm = comp.getSourceManager();
    if (!sm) return;
    for (const auto* tree : meth::userSyntaxTrees(comp)) fn(*tree, *sm);
}

std::string_view baseName(std::string_view path) {
    size_t slash = path.find_last_of("/\\");
    return slash == std::string_view::npos ? path : path.substr(slash + 1);
}

std::string_view stem(std::string_view path) {
    std::string_view b = baseName(path);
    return b.substr(0, b.find('.'));
}

} // namespace

// SV-PKG-001 ------------------------------------------------------------------------------

namespace {

struct ClassFile {
    std::string path;
    std::vector<const ClassDeclarationSyntax*> classes;
    bool hasOtherUnits = false; // modules, packages, interfaces
};

class TopLevelClassVisitor : public SyntaxVisitor<TopLevelClassVisitor> {
public:
    TopLevelClassVisitor(const slang::SourceManager& sm, std::map<std::string, ClassFile>& files,
                         std::set<std::pair<std::string, size_t>>& seen) :
        sm(sm), files(files), seen(seen) {}

    void handle(const ClassDeclarationSyntax& cls) {
        // Top level only: directly in $unit or in a package. Nested classes go with their parent.
        const SyntaxNode* up = owner(cls);
        if (up && (up->kind == SyntaxKind::CompilationUnit || up->kind == SyntaxKind::PackageDeclaration)) {
            if (auto* f = fileOf(cls.getFirstToken().location()); f) {
                if (seen.insert({f->path, cls.getFirstToken().location().offset()}).second)
                    f->classes.push_back(&cls);
            }
        }
    }

    void handle(const ModuleDeclarationSyntax& unit) {
        slang::SourceLocation loc = unit.getFirstToken().location();
        if (auto* f = fileOf(loc)) f->hasOtherUnits = true;
        visitDefault(unit);
    }

private:
    const slang::SourceManager& sm;
    std::map<std::string, ClassFile>& files;
    std::set<std::pair<std::string, size_t>>& seen;

    // Only `include'd files: they are the class files of a package. A file compiled on its
    // own that holds classes is a $unit problem (SV-MOD-001), not a naming one.
    ClassFile* fileOf(slang::SourceLocation loc) {
        if (!userFileLoc(sm, loc) || !sm.getIncludedFrom(loc.buffer()).valid()) return nullptr;
        std::string path(sm.getRawFileName(loc.buffer()));
        auto& f = files[path];
        f.path = path;
        return &f;
    }
};

} // namespace

class FileClassNameMismatch : public Rule {
public:
    std::string_view getId() const override { return "SV-PKG-001"; }
    std::string_view getName() const override { return "FileClassNameMismatch"; }
    std::string_view getDescription() const override {
        return "Flags `include'd class files that define more than one top-level class, or whose "
               "one class is not named after the file (my_drv.svh holding my_driver). With one class "
               "per file, named after it, a class can be found from its name and reused on its own "
               "(MG-1.2).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        std::map<std::string, ClassFile> files;
        std::set<std::pair<std::string, size_t>> seen;
        forEachUserTree(comp, [&](const SyntaxTree& tree, const slang::SourceManager& sm) {
            TopLevelClassVisitor v(sm, files, seen);
            tree.root().visit(v);
        });
        for (const auto& [path, f] : files) {
            if (f.classes.empty() || f.hasOtherUnits) continue;
            std::string_view fileStem = stem(path);
            std::string file(baseName(path));
            if (f.classes.size() == 1) {
                const auto& cls = *f.classes[0];
                if (cls.name.valueText() == fileStem) continue;
                diags.addViolation(
                    "SV-PKG-001", "FileClassNameMismatch",
                    "'" + file + "' holds class '" + std::string(cls.name.valueText()) +
                        "'. Name the file after the class (" + std::string(cls.name.valueText()) +
                        ".svh) so it can be found from the class name.",
                    core::Severity::Warning, cls.name.location());
                continue;
            }
            // Keep the class the file is named after (or the first one); flag the rest.
            const ClassDeclarationSyntax* keep = f.classes[0];
            for (const auto* cls : f.classes) {
                if (cls->name.valueText() == fileStem) keep = cls;
            }
            for (const auto* cls : f.classes) {
                if (cls == keep) continue;
                diags.addViolation(
                    "SV-PKG-001", "FileClassNameMismatch",
                    "'" + file + "' defines " + std::to_string(f.classes.size()) + " classes. Move '" +
                        std::string(cls->name.valueText()) + "' to its own file, " +
                        std::string(cls->name.valueText()) + ".svh, so that each class can be found "
                        "and reused on its own.",
                    core::Severity::Warning, cls->name.location());
            }
        }
    }
};

// SV-PKG-002 ------------------------------------------------------------------------------

namespace {

class WildcardImportVisitor : public SyntaxVisitor<WildcardImportVisitor> {
public:
    WildcardImportVisitor(const slang::SourceManager& sm, core::DiagnosticEngine& diags,
                          std::set<std::pair<std::string, size_t>>& seen) :
        sm(sm), diags(diags), seen(seen) {}

    void handle(const PackageImportDeclarationSyntax& decl) {
        slang::SourceLocation loc = decl.keyword.location();
        const SyntaxNode* up = owner(decl);
        if (!userFileLoc(sm, loc) || !up) return;
        if (!seen.insert({std::string(sm.getRawFileName(loc.buffer())), loc.offset()}).second) return;
        for (const auto* item : decl.items) {
            if (item->item.kind != TokenKind::Star) continue;
            std::string pkg(item->package.valueText());
            if (up->kind == SyntaxKind::CompilationUnit) {
                diags.addViolation(
                    "SV-PKG-002", "WildcardImportOutsidePackage",
                    "'import " + pkg + "::*' at file scope puts every name of " + pkg +
                        " into $unit, where it is visible to every file compiled after this one. "
                        "Move the import inside the module, interface or package that uses it.",
                    core::Severity::Warning, loc);
            } else if (up->kind == SyntaxKind::PackageDeclaration) {
                slang::SourceLocation pkgLoc = up->getFirstToken().location();
                if (pkgLoc.buffer() == loc.buffer()) continue;
                diags.addViolation(
                    "SV-PKG-002", "WildcardImportOutsidePackage",
                    "'import " + pkg + "::*' sits in a file that is `include'd into a package, so "
                        "it applies to every file included after it and the package's real "
                        "dependencies are hidden. Import in the package file itself.",
                    core::Severity::Warning, loc);
            }
        }
    }

private:
    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
    std::set<std::pair<std::string, size_t>>& seen;
};

} // namespace

class WildcardImportOutsidePackage : public Rule {
public:
    std::string_view getId() const override { return "SV-PKG-002"; }
    std::string_view getName() const override { return "WildcardImportOutsidePackage"; }
    std::string_view getDescription() const override {
        return "Flags 'import pkg::*' at file ($unit) scope, and inside files that are `include'd "
               "into a package. Both leak names into code that never asked for them and hide where "
               "a dependency comes from (information hiding, MG-1.3).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        std::set<std::pair<std::string, size_t>> seen;
        forEachUserTree(comp, [&](const SyntaxTree& tree, const slang::SourceManager& sm) {
            WildcardImportVisitor v(sm, diags, seen);
            tree.root().visit(v);
        });
    }
};

// Package import graph, shared by SV-PKG-003 to SV-PKG-005 --------------------------------

namespace {

struct Import {
    const Symbol* symbol = nullptr; // WildcardImportSymbol or ExplicitImportSymbol
    const Scope* scope = nullptr;   // where the import is
    std::string_view package;       // imported package
    bool wildcard = false;
};

class ImportCollector : public ASTVisitor<ImportCollector, VisitFlags::AllGood> {
public:
    ImportCollector(const slang::SourceManager& sm, std::vector<Import>& out,
                    std::unordered_set<const Symbol*>& seen) : sm(sm), out(out), seen(seen) {}

    void handle(const WildcardImportSymbol& s) { add(s, s.packageName, true); }
    void handle(const ExplicitImportSymbol& s) { add(s, s.packageName, false); }
    // Imports live in packages, modules, interfaces and $unit; class bodies and procedural
    // code need not be walked.
    void handle(const ClassType&) {}
    void handle(const GenericClassDefSymbol&) {}
    void handle(const SubroutineSymbol&) {}
    void handle(const ProceduralBlockSymbol&) {}

private:
    const slang::SourceManager& sm;
    std::vector<Import>& out;
    std::unordered_set<const Symbol*>& seen;

    void add(const Symbol& s, std::string_view pkg, bool wildcard) {
        if (!meth::isUserLocation(sm, s.location) || !seen.insert(&s).second) return;
        out.push_back({&s, s.getParentScope(), pkg, wildcard});
    }
};

std::vector<Import> userImports(Compilation& comp) {
    std::vector<Import> out;
    const auto* sm = comp.getSourceManager();
    if (!sm) return out;
    std::unordered_set<const Symbol*> seen;
    ImportCollector v(*sm, out, seen);
    for (const auto* unit : comp.getCompilationUnits()) unit->visit(v);
    for (const auto* pkg : comp.getPackages()) {
        if (!swe::isLibrarySymbol(*pkg)) pkg->visit(v);
    }
    comp.getRoot().visit(v);
    return out;
}

const PackageSymbol* userPackage(Compilation& comp, std::string_view name) {
    const auto* pkg = comp.getPackage(name);
    if (!pkg || swe::isLibrarySymbol(*pkg)) return nullptr;
    const auto* sm = comp.getSourceManager();
    return sm && meth::isUserLocation(*sm, pkg->location) ? pkg : nullptr;
}

/// Edges between user packages: the importing package, the imported one, and the first
/// import that makes the edge.
struct PackageGraph {
    std::vector<const PackageSymbol*> packages;
    std::map<std::pair<const PackageSymbol*, const PackageSymbol*>, const Symbol*> edges;
    std::map<const PackageSymbol*, int> afferent, efferent;

    double instability(const PackageSymbol* p) const {
        int ca = afferent.count(p) ? afferent.at(p) : 0;
        int ce = efferent.count(p) ? efferent.at(p) : 0;
        return ca + ce == 0 ? 0.0 : double(ce) / double(ca + ce);
    }
};

PackageGraph packageGraph(Compilation& comp) {
    PackageGraph g;
    for (const auto& imp : userImports(comp)) {
        if (!imp.scope || imp.scope->asSymbol().kind != SymbolKind::Package) continue;
        const auto* from = &imp.scope->asSymbol().as<PackageSymbol>();
        const auto* to = userPackage(comp, imp.package);
        if (!to || to == from || !userPackage(comp, from->name)) continue;
        if (g.edges.try_emplace({from, to}, imp.symbol).second) {
            g.efferent[from]++;
            g.afferent[to]++;
        }
    }
    std::set<const PackageSymbol*> seen;
    for (const auto& [edge, _] : g.edges) {
        for (const auto* p : {edge.first, edge.second}) {
            if (seen.insert(p).second) g.packages.push_back(p);
        }
    }
    return g;
}

} // namespace

// SV-PKG-003 ------------------------------------------------------------------------------

class UnusedPackageImport : public Rule {
public:
    std::string_view getId() const override { return "SV-PKG-003"; }
    std::string_view getName() const override { return "UnusedPackageImport"; }
    std::string_view getDescription() const override {
        return "Flags package imports that supply no name used in their scope. An unused import is "
               "a false dependency: the package is compiled first, changes to it look relevant, and "
               "names it exports can collide with local ones (common reuse principle, MG-1.3).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        for (const auto& imp : userImports(comp)) {
            const auto* syntax = imp.symbol->getSyntax();
            if (!syntax || comp.isReferenced(*syntax).first) continue;
            // A package that does not exist is a compile error, not an unused import.
            if (!comp.getPackage(imp.package)) continue;
            std::string what = imp.wildcard ? std::string(imp.package) + "::*"
                                            : std::string(imp.package) + "::" + std::string(imp.symbol->name);
            std::string where = imp.scope ? std::string(imp.scope->asSymbol().name) : std::string();
            if (where.empty()) where = "$unit";
            diags.addViolation(
                "SV-PKG-003", "UnusedPackageImport",
                "'import " + what + "' in " + where + " supplies nothing that is used there. Remove "
                    "it, so the scope's dependencies are the ones it really has.",
                core::Severity::Warning, imp.symbol->location);
        }
    }
};

// SV-PKG-004 ------------------------------------------------------------------------------

namespace {

int layerOf(swe::Role role) {
    switch (role) {
        case swe::Role::Driver:
        case swe::Role::Monitor:
        case swe::Role::Sequencer:
        case swe::Role::Agent: return 1;
        case swe::Role::Env:
        case swe::Role::Scoreboard: return 2;
        case swe::Role::Test: return 3;
        default: return 0;
    }
}

const char* layerName(int layer) {
    switch (layer) {
        case 1: return "agent";
        case 2: return "env";
        case 3: return "test";
        default: return "object";
    }
}

/// Highest testbench layer of the classes a package defines (0 when it has no components).
std::map<const PackageSymbol*, int> packageLayers(const uvm::UvmModel& model) {
    std::map<const PackageSymbol*, int> out;
    for (const auto* cls : swe::userClasses(model)) {
        const Scope* s = cls->getParentScope();
        if (!s || s->asSymbol().kind != SymbolKind::Package) continue;
        int& l = out[&s->asSymbol().as<PackageSymbol>()];
        l = std::max(l, layerOf(swe::classRole(*cls)));
    }
    return out;
}

} // namespace

class PackageLayerInversion : public Rule {
public:
    std::string_view getId() const override { return "SV-PKG-004"; }
    std::string_view getName() const override { return "PackageLayerInversion"; }
    std::string_view getDescription() const override {
        return "Flags a package that imports a package from a higher testbench layer, such as an "
               "agent package importing an env or test package. The lower package can then not be "
               "compiled or reused without the higher one (acyclic and stable dependencies, MG-1.1). "
               "UVM-ARC-001 checks the same order between classes.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        auto layers = packageLayers(model);
        auto layer = [&](const PackageSymbol* p) { return layers.count(p) ? layers.at(p) : 0; };
        PackageGraph g = packageGraph(comp);
        for (const auto& [edge, imp] : g.edges) {
            auto [from, to] = edge;
            int lf = layer(from), lt = layer(to);
            if (lf == 0 || lt <= lf) continue;
            diags.addViolation(
                "SV-PKG-004", "PackageLayerInversion",
                std::string(layerName(lf)) + " package '" + std::string(from->name) + "' imports " +
                    layerName(lt) + " package '" + std::string(to->name) + "'. Dependencies must point "
                    "down the stack (item, sequence, agent, env, test); otherwise the " + layerName(lf) +
                    " cannot be reused without the " + layerName(lt) + ". Move what is shared into "
                    "the lower package or a common one.",
                core::Severity::Warning, imp->location);
        }
    }
};

// SV-PKG-005 ------------------------------------------------------------------------------

class UnstablePackageDependedOn : public Rule {
public:
    std::string_view getId() const override { return "SV-PKG-005"; }
    std::string_view getName() const override { return "UnstablePackageDependedOn"; }
    std::string_view getDescription() const override {
        return "Computes instability I = Ce / (Ca + Ce) for each user package from its imports and "
               "flags a package that depends on a less stable one. Many packages rely on a stable "
               "package, so its own dependencies should be at least as stable (Martin's stable "
               "dependencies principle, MG-1.4). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        PackageGraph g = packageGraph(comp);
        auto fmt = [](double v) {
            char buf[16];
            std::snprintf(buf, sizeof buf, "%.2f", v);
            return std::string(buf);
        };
        for (const auto& [edge, imp] : g.edges) {
            auto [from, to] = edge;
            double iFrom = g.instability(from), iTo = g.instability(to);
            if (iFrom >= iTo) continue;
            diags.addViolation(
                "SV-PKG-005", "UnstablePackageDependedOn",
                "Package '" + std::string(from->name) + "' (instability " + fmt(iFrom) + ", used by " +
                    std::to_string(g.afferent[from]) + " packages) depends on '" + std::string(to->name) +
                    "' (instability " + fmt(iTo) + "), which changes more easily. A change there "
                    "ripples into everything that uses '" + std::string(from->name) +
                    "'. Invert the dependency or move the shared part into a stable package.",
                core::Severity::Info, imp->location);
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createFileClassNameMismatch() { return std::make_unique<methodology::FileClassNameMismatch>(); }
std::unique_ptr<Rule> createWildcardImportOutsidePackage() { return std::make_unique<methodology::WildcardImportOutsidePackage>(); }
std::unique_ptr<Rule> createUnusedPackageImport() { return std::make_unique<methodology::UnusedPackageImport>(); }
std::unique_ptr<Rule> createPackageLayerInversion() { return std::make_unique<methodology::PackageLayerInversion>(); }
std::unique_ptr<Rule> createUnstablePackageDependedOn() { return std::make_unique<methodology::UnstablePackageDependedOn>(); }
}
