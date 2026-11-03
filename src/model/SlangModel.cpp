#include "uvmcheck/model/SlangModel.h"

#include <unordered_map>

#include "slang/ast/Compilation.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"
#include "slang/text/SourceManager.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::model {

namespace {

namespace sa = slang::ast;

Visibility toVisibility(sa::Visibility v) {
    switch (v) {
        case sa::Visibility::Protected: return Visibility::Protected;
        case sa::Visibility::Local: return Visibility::Local;
        default: return Visibility::Public;
    }
}

UvmKind classify(const uvm::UvmModel& uvm, const sa::Type& t) {
    // Component and sequence checks come first: a reduced UVM (a test mock)
    // need not derive them from uvm_object.
    if (uvm.isUvmComponent(t)) {
        // Most specific first: a test is also a component.
        static constexpr std::pair<std::string_view, UvmKind> roles[] = {
            {"uvm_test", UvmKind::Test},
            {"uvm_env", UvmKind::Env},
            {"uvm_agent", UvmKind::Agent},
            {"uvm_driver", UvmKind::Driver},
            {"uvm_monitor", UvmKind::Monitor},
            {"uvm_sequencer_base", UvmKind::Sequencer},
            {"uvm_scoreboard", UvmKind::Scoreboard},
            {"uvm_subscriber", UvmKind::Subscriber},
        };
        for (const auto& [base, kind] : roles) {
            if (uvm.isDerivedFrom(t, base)) return kind;
        }
        return UvmKind::Component;
    }
    if (uvm.isUvmSequence(t)) return UvmKind::Sequence;
    if (uvm.isUvmSequenceItem(t)) return UvmKind::SequenceItem;
    return uvm.isUvmObject(t) ? UvmKind::Object : UvmKind::None;
}

class Builder {
public:
    Builder(const sa::Compilation& c, const uvm::UvmModel& u, const BuildOptions& o)
        : compilation(c), sm(c.getSourceManager()), uvm(u), options(o) {}

    Model run() {
        // Ids first, so bases and property types can point at any class:
        // the plain classes the UVM model found, then one entry per
        // parameterized class definition (uvm_driver #(REQ, RSP) and the like).
        for (const auto* ct : uvm.getAllClasses()) {
            if (ct) ids.try_emplace(ct, static_cast<ClassId>(ids.size()));
        }
        // getRoot() is not const in slang, but on an elaborated design it only
        // returns the finished root.
        collectGenerics(const_cast<sa::Compilation&>(compilation).getRoot());
        const auto firstGeneric = static_cast<ClassId>(ids.size());
        for (size_t i = 0; i < generics.size(); ++i)
            genericIds.emplace(generics[i], static_cast<ClassId>(firstGeneric + i));

        for (const auto* ct : uvm.getAllClasses()) {
            if (ct && ids.at(ct) == model.classes().size()) model.addClass(makeClass(*ct));
        }
        for (const auto* g : generics) model.addClass(makeGeneric(*g));

        // A definition nothing specialized has no type to read its base from;
        // its base was taken from the source text, so resolve it by name.
        for (ClassId id = firstGeneric; id < model.classes().size(); ++id) {
            Class& c = model.mutableClass(id);
            if (c.base == NoClass && !c.baseName.empty()) {
                if (auto b = model.findClass(c.baseName); b && *b != id) c.base = *b;
            }
        }
        return std::move(model);
    }

private:
    const sa::Compilation& compilation;
    const slang::SourceManager* sm;
    const uvm::UvmModel& uvm;
    const BuildOptions& options;
    Model model;
    std::unordered_map<const sa::ClassType*, ClassId> ids;
    std::vector<const sa::GenericClassDefSymbol*> generics;
    std::unordered_map<const sa::GenericClassDefSymbol*, ClassId> genericIds;

    // Same walk as UvmModel::indexScope: packages, $unit, nested classes.
    void collectGenerics(const sa::Scope& scope) {
        for (const auto& member : scope.members()) {
            switch (member.kind) {
                case sa::SymbolKind::GenericClassDef:
                    generics.push_back(&member.as<sa::GenericClassDefSymbol>());
                    break;
                case sa::SymbolKind::ClassType:
                    collectGenerics(member.as<sa::ClassType>());
                    break;
                case sa::SymbolKind::Package:
                    collectGenerics(member.as<sa::PackageSymbol>());
                    break;
                case sa::SymbolKind::CompilationUnit:
                    collectGenerics(member.as<sa::CompilationUnitSymbol>());
                    break;
                default:
                    break;
            }
        }
    }

    /// One existing specialization to read members from, chosen by its
    /// printed type so the choice does not depend on hash order. Never makes
    /// a new specialization: the design is already elaborated and shared.
    static const sa::ClassType* representative(const sa::GenericClassDefSymbol& g) {
        const sa::ClassType* best = nullptr;
        std::string bestName;
        for (const auto& spec : g.specializations()) {
            const auto& canon = spec.getCanonicalType();
            if (!canon.isClass()) continue;
            std::string name = canon.toString();
            if (!best || name < bestName) {
                best = &canon.as<sa::ClassType>();
                bestName = std::move(name);
            }
        }
        return best;
    }

    static std::string baseNameFromSyntax(const sa::GenericClassDefSymbol& g) {
        const auto* syntax = g.getSyntax();
        if (!syntax || syntax->kind != slang::syntax::SyntaxKind::ClassDeclaration) return {};
        const auto& decl = syntax->as<slang::syntax::ClassDeclarationSyntax>();
        if (!decl.extendsClause) return {};
        std::string text = decl.extendsClause->baseName->toString();
        if (auto hash = text.find('#'); hash != std::string::npos) text.erase(hash);
        if (auto colon = text.rfind("::"); colon != std::string::npos) text.erase(0, colon + 2);
        while (!text.empty() && (text.back() == ' ' || text.back() == '\n' || text.back() == '\t')) text.pop_back();
        while (!text.empty() && (text.front() == ' ' || text.front() == '\n' || text.front() == '\t')) text.erase(0, 1);
        return text;
    }

    Class makeGeneric(const sa::GenericClassDefSymbol& g) {
        const sa::ClassType* rep = representative(g);
        Class c = rep ? makeClass(*rep) : Class{};
        c.name = std::string(g.name);
        c.loc = loc(g.location);
        c.scope.clear();
        if (const auto* parent = g.getParentScope()) {
            const auto& sym = parent->asSymbol();
            if (sym.kind == sa::SymbolKind::Package || sym.kind == sa::SymbolKind::ClassType)
                c.scope = std::string(sym.name);
        }
        c.isParameterized = true;
        if (!rep) {
            c.baseName = baseNameFromSyntax(g);
            if (options.isLibraryFile) c.isLibrary = options.isLibraryFile(model.fileName(c.loc));
        }
        return c;
    }

    Loc loc(slang::SourceLocation l) {
        Loc out;
        if (!l.valid()) return out;
        out.raw = (uint64_t(l.buffer().getId()) << 32) | uint64_t(l.offset() & 0xffffffffu);
        if (!sm) return out;
        // Code from a macro (`uvm_component_utils and the like) is reported
        // where the user wrote it, as findings are.
        const auto orig = sm->getFullyOriginalLoc(l);
        out.file = model.internFile(sm->getFileName(orig));
        out.line = static_cast<uint32_t>(sm->getLineNumber(orig));
        out.column = static_cast<uint32_t>(sm->getColumnNumber(orig));
        return out;
    }

    ClassId idOf(const sa::Type& t) const {
        const auto& canon = t.getCanonicalType();
        if (!canon.isClass()) return NoClass;
        const auto& ct = canon.as<sa::ClassType>();
        if (auto it = ids.find(&ct); it != ids.end()) return it->second;
        // A specialization maps to its parameterized definition's entry.
        if (ct.genericClass) {
            if (auto it = genericIds.find(ct.genericClass); it != genericIds.end()) return it->second;
        }
        return NoClass;
    }

    Class makeClass(const sa::ClassType& ct) {
        Class c;
        c.name = std::string(ct.name);
        c.loc = loc(ct.location);
        if (const auto* parent = ct.getParentScope()) {
            const auto& sym = parent->asSymbol();
            if (sym.kind == sa::SymbolKind::Package || sym.kind == sa::SymbolKind::ClassType)
                c.scope = std::string(sym.name);
        }
        c.isVirtual = ct.isAbstract;
        if (const auto* base = ct.getBaseClass(); base && !base->isError()) {
            const auto& canon = base->getCanonicalType();
            if (canon.isClass()) {
                const auto& bct = canon.as<sa::ClassType>();
                c.baseName = std::string(bct.genericClass ? bct.genericClass->name : bct.name);
                c.base = idOf(canon);
            }
        }
        c.uvm = classify(uvm, ct);
        if (options.isLibraryFile) c.isLibrary = options.isLibraryFile(model.fileName(c.loc));

        for (const auto& member : ct.members()) {
            if (member.kind == sa::SymbolKind::ClassProperty) {
                const auto& p = member.as<sa::ClassPropertySymbol>();
                Property prop;
                prop.name = std::string(p.name);
                prop.type = p.getType().toString();
                prop.loc = loc(p.location);
                prop.visibility = toVisibility(p.visibility);
                prop.isStatic = p.lifetime == sa::VariableLifetime::Static;
                prop.isConst = p.flags.has(sa::VariableFlags::Const);
                prop.isRand = p.randMode != sa::RandMode::None;
                prop.typeClass = idOf(p.getType());
                c.properties.push_back(std::move(prop));
            } else if (member.kind == sa::SymbolKind::Subroutine) {
                const auto& s = member.as<sa::SubroutineSymbol>();
                Method m;
                m.name = std::string(s.name);
                m.loc = loc(s.location);
                m.visibility = toVisibility(s.visibility);
                m.isTask = s.subroutineKind == sa::SubroutineKind::Task;
                m.isVirtual = s.isVirtual();
                m.isPure = s.flags.has(sa::MethodFlags::Pure);
                m.isStatic = s.flags.has(sa::MethodFlags::Static);
                if (uvm::UvmModel::isFunctionPhase(s.name)) m.phase = PhaseKind::Function;
                else if (uvm::UvmModel::isTaskPhase(s.name)) m.phase = PhaseKind::Task;
                c.methods.push_back(std::move(m));
            }
        }
        return c;
    }
};

} // namespace

Model buildFromSlang(const slang::ast::Compilation& compilation, const uvm::UvmModel& uvm,
                     const BuildOptions& options) {
    return Builder(compilation, uvm, options).run();
}

slang::SourceLocation toSlang(const Loc& loc) {
    if (loc.raw == 0) return slang::SourceLocation::NoLocation;
    return slang::SourceLocation(slang::BufferID(static_cast<uint32_t>(loc.raw >> 32), {}),
                                 loc.raw & 0xffffffffu);
}

} // namespace uvmcheck::model
