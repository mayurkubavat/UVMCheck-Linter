// Methodology guide chapter MG-7, duplication: SV-DUP-002 to SV-DUP-005. SV-DUP-001 (identical
// method bodies across classes) lives in oop/DuplicateMethodBody.cpp.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/InstanceSymbols.h"
#include "slang/syntax/AllSyntax.h"

#include <algorithm>
#include <map>
#include <set>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

std::string where(Compilation& comp, slang::SourceLocation loc) {
    const auto* sm = comp.getSourceManager();
    if (!sm || !loc.valid()) return {};
    loc = sm->getFullyExpandedLoc(loc);
    std::string_view path = sm->getRawFileName(loc.buffer());
    if (size_t slash = path.find_last_of("/\\"); slash != std::string_view::npos) path.remove_prefix(slash + 1);
    return std::string(path) + ":" + std::to_string(sm->getLineNumber(loc));
}

std::string routineName(const SubroutineSymbol& sub) {
    const Scope* s = sub.getParentScope();
    if (s && s->asSymbol().kind == SymbolKind::ClassType)
        return std::string(meth::plainName(s->asSymbol().as<ClassType>())) + "::" + std::string(sub.name);
    return std::string(sub.name);
}

/// Written in a generated file; cached per buffer for one run.
class GeneratedFiles {
public:
    explicit GeneratedFiles(const slang::SourceManager& sm) : sm(sm) {}
    bool at(slang::SourceLocation loc) {
        if (!loc.valid()) return false;
        loc = sm.getFullyOriginalLoc(loc);
        auto [it, fresh] = cache.try_emplace(loc.buffer().getId(), false);
        if (fresh) it->second = meth::isGeneratedBuffer(sm, loc.buffer());
        return it->second;
    }

private:
    const slang::SourceManager& sm;
    std::unordered_map<uint32_t, bool> cache;
};

} // namespace

// SV-DUP-002 ------------------------------------------------------------------------------

namespace {

struct Stmt {
    const Statement* stmt = nullptr;
    std::vector<meth::NormToken> tokens;
    std::string shape; // tokenKey of this statement alone, with renaming
};

struct StmtList {
    const SubroutineSymbol* routine = nullptr;
    std::vector<Stmt> stmts;
    bool wholeBody = false;
};

class ListCollector : public ASTVisitor<ListCollector, VisitFlags::AllGood> {
public:
    ListCollector(const slang::SourceManager& sm, const SubroutineSymbol& sub, std::vector<StmtList>& out) :
        sm(sm), sub(sub), out(out) {}

    void handle(const StatementList& list) {
        StmtList l;
        l.routine = &sub;
        l.wholeBody = &list == &sub.getBody() ||
                      (sub.getBody().kind == StatementKind::Block &&
                       &sub.getBody().as<BlockStatement>().body == &list);
        // A list that a macro expanded to (`uvm_info's begin ... end) was written once, in the
        // macro; only lists with statements written in this routine's file count.
        slang::BufferID home = sm.getFullyOriginalLoc(sub.location).buffer();
        bool writtenHere = false;
        for (const auto* s : list.list) {
            if (s && sm.getFullyOriginalLoc(swe::locOf(*s)).buffer() == home) writtenHere = true;
        }
        if (!writtenHere) {
            visitDefault(list);
            return;
        }
        for (const auto* s : list.list) {
            if (!s || !s->syntax) continue;
            Stmt st{s, {}};
            meth::writtenTokens(sm, *s->syntax, st.tokens);
            if (st.tokens.empty()) continue;
            st.shape = meth::tokenKey(st.tokens, 0, st.tokens.size(), true);
            l.stmts.push_back(std::move(st));
        }
        if (l.stmts.size() >= 2) out.push_back(std::move(l));
        visitDefault(list);
    }

private:
    const slang::SourceManager& sm;
    const SubroutineSymbol& sub;
    std::vector<StmtList>& out;
};

struct Window {
    size_t list = 0, first = 0, last = 0; // statement indices, inclusive
};

std::vector<meth::NormToken> windowTokens(const StmtList& l, size_t first, size_t last) {
    std::vector<meth::NormToken> out;
    for (size_t i = first; i <= last; i++) out.insert(out.end(), l.stmts[i].tokens.begin(), l.stmts[i].tokens.end());
    return out;
}

/// The window repeats a short cycle of statement shapes, like an unrolled loop or a run of
/// register writes. It matches itself shifted by one cycle, which is not a copied fragment.
bool periodic(const StmtList& l, size_t first, size_t last) {
    size_t len = last - first + 1;
    for (size_t p = 1; p <= len / 2; p++) {
        bool cycle = true;
        for (size_t k = first; k + p <= last && cycle; k++) cycle = l.stmts[k].shape == l.stmts[k + p].shape;
        if (cycle) return true;
    }
    return false;
}

std::string windowKey(const StmtList& l, size_t first, size_t last, bool rename) {
    auto toks = windowTokens(l, first, last);
    return meth::tokenKey(toks, 0, toks.size(), rename);
}

} // namespace

class CloneFragments : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-002"; }
    std::string_view getName() const override { return "CloneFragments"; }
    std::string_view getDescription() const override {
        return "Finds runs of three or more statements (at least 50 tokens) that appear in more than "
               "one place, also when variables and literals were renamed consistently. Macro calls "
               "count as written, not as their expansion, and called names must match; runs that "
               "repeat one short cycle of statements, like unrolled loops, are left out. Each pair of "
               "routines is reported once; whole identical method bodies are SV-DUP-001's, and files "
               "whose header says they are generated are left out (DRY, Type-1/2 clones, MG-7.1).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        size_t minTokens = static_cast<size_t>(swe::ruleOption("SV-DUP-002", "min_tokens", 50));
        size_t minStmts = static_cast<size_t>(swe::ruleOption("SV-DUP-002", "min_statements", 3));

        GeneratedFiles generated(*sm);
        std::vector<StmtList> lists;
        for (const auto* sub : swe::allUserRoutines(comp, model)) {
            if (!meth::isUserLocation(*sm, sub->location) || generated.at(sub->location)) continue;
            ListCollector c(*sm, *sub, lists);
            sub->getBody().visit(c);
        }

        // Smallest window starting at each statement that reaches both minimums.
        std::unordered_map<std::string, std::vector<Window>> byKey;
        std::vector<std::pair<Window, std::string>> windows;
        for (size_t li = 0; li < lists.size(); li++) {
            const auto& l = lists[li];
            for (size_t i = 0; i < l.stmts.size(); i++) {
                size_t tokens = 0, j = i;
                for (; j < l.stmts.size(); j++) {
                    tokens += l.stmts[j].tokens.size();
                    if (tokens >= minTokens && j - i + 1 >= minStmts) break;
                }
                if (j == l.stmts.size()) break;
                if (periodic(l, i, j)) continue;
                std::string key = windowKey(l, i, j, true);
                byKey[key].push_back({li, i, j});
                windows.push_back({{li, i, j}, std::move(key)});
            }
        }

        auto overlaps = [&](const Window& a, const Window& b) {
            return lists[a.list].routine == lists[b.list].routine && a.list == b.list &&
                   !(a.last < b.first || b.last < a.first);
        };
        std::set<std::pair<const SubroutineSymbol*, const SubroutineSymbol*>> reported;
        std::unordered_map<size_t, size_t> coveredUntil; // list -> last statement already reported
        for (const auto& [w, key] : windows) {
            if (coveredUntil.count(w.list) && w.first <= coveredUntil[w.list]) continue;
            const Window* orig = nullptr;
            for (const auto& o : byKey[key]) {
                if (o.list == w.list && o.first == w.first) break; // only earlier occurrences
                if (!overlaps(o, w)) { orig = &o; break; }
            }
            if (!orig) continue;
            const auto& lo = lists[orig->list];
            const auto& lw = lists[w.list];
            auto pair = std::make_pair(lo.routine, lw.routine);
            if (reported.count(pair)) continue;

            // Grow the match while both sides go on the same way.
            Window a = *orig, b = w;
            while (a.last + 1 < lo.stmts.size() && b.last + 1 < lw.stmts.size() &&
                   !(a.list == b.list && a.last + 1 >= b.first) &&
                   windowKey(lo, a.first, a.last + 1, true) == windowKey(lw, b.first, b.last + 1, true)) {
                a.last++;
                b.last++;
            }
            bool whole = lo.wholeBody && lw.wholeBody && a.first == 0 && b.first == 0 &&
                         a.last + 1 == lo.stmts.size() && b.last + 1 == lw.stmts.size();
            if (whole && windowKey(lo, a.first, a.last, false) == windowKey(lw, b.first, b.last, false)) continue;

            reported.insert(pair);
            coveredUntil[b.list] = b.last;
            size_t tokens = windowTokens(lw, b.first, b.last).size();
            size_t count = b.last - b.first + 1;
            diags.addViolation(
                "SV-DUP-002", "CloneFragments",
                std::to_string(count) + " statements (" + std::to_string(tokens) + " tokens) in '" +
                    routineName(*lw.routine) + "' repeat code in '" + routineName(*lo.routine) + "' (" +
                    where(comp, swe::locOf(*lo.stmts[a.first].stmt)) + "), apart from names and "
                    "literals. A fix in one copy will be missed in the other; move the shared steps "
                    "into a helper routine or a common base class.",
                core::Severity::Warning, swe::locOf(*lw.stmts[b.first].stmt));
        }
    }
};

// SV-DUP-003 ------------------------------------------------------------------------------

class DuplicatedConstraint : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-003"; }
    std::string_view getName() const override { return "DuplicatedConstraint"; }
    std::string_view getDescription() const override {
        return "Flags a constraint body repeated in two classes that do not inherit from each other. "
               "The rule then lives in two places and drifts; declare it once in a shared base class "
               "or a policy object (Fowler 'pull up', MG-7.1). SV-RND-011 covers repeats inside one "
               "class.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        size_t minTokens = static_cast<size_t>(swe::ruleOption("SV-DUP-003", "min_tokens", 8));
        struct First {
            const ClassType* cls;
            const Symbol* constraint;
        };
        std::unordered_map<std::string, std::vector<First>> seen;
        GeneratedFiles generated(*sm);
        for (const auto* cls : swe::userClasses(model)) {
            if (generated.at(cls->location)) continue;
            for (const auto& member : cls->members()) {
                if (member.kind != SymbolKind::ConstraintBlock) continue;
                const auto* syntax = member.getSyntax();
                if (!syntax || syntax->kind != slang::syntax::SyntaxKind::ConstraintDeclaration) continue;
                std::vector<meth::NormToken> toks;
                meth::writtenTokens(*sm, *syntax->as<slang::syntax::ConstraintDeclarationSyntax>().block, toks);
                if (toks.size() < minTokens + 2) continue; // + the braces
                std::string key = meth::tokenKey(toks, 0, toks.size(), false);
                auto& firsts = seen[key];
                const First* match = nullptr;
                for (const auto& f : firsts) {
                    if (f.cls != cls && !swe::derivesFrom(*cls, *f.cls) && !swe::derivesFrom(*f.cls, *cls)) {
                        match = &f;
                        break;
                    }
                }
                if (match) {
                    diags.addViolation(
                        "SV-DUP-003", "DuplicatedConstraint",
                        "Constraint '" + std::string(member.name) + "' in '" + std::string(meth::plainName(*cls)) +
                            "' repeats constraint '" + std::string(match->constraint->name) + "' of '" +
                            std::string(meth::plainName(*match->cls)) + "' (" +
                            where(comp, match->constraint->location) + "). Declare it once, in a base "
                            "class both extend or in a shared constraint policy, so a change reaches "
                            "both.",
                        core::Severity::Warning, member.location);
                } else {
                    firsts.push_back({cls, &member});
                }
            }
        }
    }
};

// SV-DUP-004 ------------------------------------------------------------------------------

namespace {

class StringLiteralCollector : public ASTVisitor<StringLiteralCollector, VisitFlags::AllGood> {
public:
    struct Use {
        std::string value;
        slang::SourceLocation loc;
    };

    StringLiteralCollector(const slang::SourceManager& sm, std::vector<Use>& uses) :
        sm(sm), uses(uses), generated(sm) {}

    void handle(const CallExpression& call) {
        // Config keys are UVM-CFG-008's; instance names passed to create()/new are each used once.
        if ((meth::isConfigDbCall(call, "set") || meth::isConfigDbCall(call, "get")) && call.arguments().size() >= 3) {
            skip.insert(&swe::stripConversions(*call.arguments()[2]));
        }
        if (meth::isFactoryCreate(call) && !call.arguments().empty()) {
            skip.insert(&swe::stripConversions(*call.arguments()[0]));
        }
        visitDefault(call);
    }

    void handle(const NewClassExpression& e) {
        if (const auto* ctor = e.constructorCall(); ctor && ctor->kind == ExpressionKind::Call) {
            const auto& call = ctor->as<CallExpression>();
            if (!call.arguments().empty()) skip.insert(&swe::stripConversions(*call.arguments()[0]));
        }
        visitDefault(e);
    }

    void handle(const StringLiteral& s) {
        if (skip.count(&s)) return;
        slang::SourceLocation loc = sm.getFullyOriginalLoc(s.sourceRange.start());
        if (!meth::isUserLocation(sm, loc) || generated.at(loc)) return;
        if (!seen.insert({loc.buffer().getId(), loc.offset()}).second) return; // one per place written
        uses.push_back({std::string(s.getValue()), loc});
    }

private:
    const slang::SourceManager& sm;
    std::vector<Use>& uses;
    std::unordered_set<const Expression*> skip;
    std::set<std::pair<uint32_t, size_t>> seen;
    GeneratedFiles generated;
};

bool worthNaming(const std::string& v) {
    if (v.size() < 4) return false;
    size_t letters = 0;
    for (char c : v) {
        if (std::isalpha(static_cast<unsigned char>(c))) letters++;
    }
    // Pure format strings ("%0d", "%s: %h") say nothing a constant could name.
    if (letters < 3) return false;
    std::string stripped;
    for (size_t i = 0; i < v.size(); i++) {
        if (v[i] == '%') {
            while (i + 1 < v.size() && !std::isalpha(static_cast<unsigned char>(v[i + 1]))) i++;
            i++;
            continue;
        }
        if (std::isalpha(static_cast<unsigned char>(v[i]))) stripped += v[i];
    }
    return stripped.size() >= 3;
}

} // namespace

class RepeatedStringLiteral : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-004"; }
    std::string_view getName() const override { return "RepeatedStringLiteral"; }
    std::string_view getDescription() const override {
        return "Flags a string literal, such as a report ID or a file name, written four or more times "
               "in the code. Config keys (UVM-CFG-008), instance names and class names are left out. "
               "One named constant keeps the copies in step (DRY, MG-7.1). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        size_t minCount = static_cast<size_t>(swe::ruleOption("SV-DUP-004", "min_count", 4));
        std::vector<StringLiteralCollector::Use> uses;
        StringLiteralCollector v(*sm, uses);
        for (const auto* sub : swe::allUserRoutines(comp, model)) sub->getBody().visit(v);
        for (const auto* inst : comp.getRoot().topInstances) inst->body.visit(v);

        std::unordered_set<std::string> classNames;
        for (const auto* cls : swe::userClasses(model)) classNames.insert(std::string(meth::plainName(*cls)));

        std::map<std::string, std::vector<slang::SourceLocation>> byValue;
        for (const auto& u : uses) {
            if (worthNaming(u.value) && !classNames.count(u.value)) byValue[u.value].push_back(u.loc);
        }
        for (const auto& [value, locs] : byValue) {
            if (locs.size() < minCount) continue;
            std::set<std::string> files;
            for (auto loc : locs) files.insert(std::string(sm->getRawFileName(loc.buffer())));
            diags.addViolation(
                "SV-DUP-004", "RepeatedStringLiteral",
                "The string \"" + value + "\" is written " + std::to_string(locs.size()) + " times in " +
                    std::to_string(files.size()) + (files.size() == 1 ? " file" : " files") +
                    ". Declare it once as a localparam string or static const in the package, so a "
                    "change or a typo cannot split the copies.",
                core::Severity::Info, locs.front());
        }
    }
};

// SV-DUP-005 ------------------------------------------------------------------------------

namespace {

/// A class's shape: its role, property count and method bodies with renaming.
std::string classShape(const slang::SourceManager& sm, const ClassType& cls, size_t& tokens) {
    std::vector<std::string> bodies;
    tokens = 0;
    for (const auto* m : swe::classMethods(cls)) {
        const auto* syntax = m->getSyntax();
        if (!syntax) continue;
        std::vector<meth::NormToken> toks;
        meth::writtenTokens(sm, *syntax, toks);
        tokens += toks.size();
        bodies.push_back(meth::tokenKey(toks, 0, toks.size(), true));
    }
    std::sort(bodies.begin(), bodies.end());
    std::string shape = std::string(swe::roleName(swe::classRole(cls))) + "|" +
                        std::to_string(meth::ownProperties(cls).size());
    for (const auto& b : bodies) shape += "|" + b;
    return shape;
}

} // namespace

class CopiedAgentPackage : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-005"; }
    std::string_view getName() const override { return "CopiedAgentPackage"; }
    std::string_view getDescription() const override {
        return "Flags two packages whose classes have the same roles, fields and method bodies apart "
               "from renamed identifiers, for at least three classes and 80% of the classes in the "
               "larger package (classes with almost no code are not counted). They are one agent "
               "copied and renamed; a parameterized agent, or a shared base with small "
               "subclasses, keeps one copy to fix (DRY, MG-7.2). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        size_t minClasses = static_cast<size_t>(swe::ruleOption("SV-DUP-005", "min_classes", 3));
        int64_t minPercent = swe::ruleOption("SV-DUP-005", "min_similarity", 80);
        size_t minTokens = static_cast<size_t>(swe::ruleOption("SV-DUP-005", "min_class_tokens", 12));

        std::vector<const PackageSymbol*> order;
        std::map<const PackageSymbol*, std::multiset<std::string>> shapes;
        std::map<const PackageSymbol*, size_t> classCount;
        GeneratedFiles generated(*sm);
        for (const auto* cls : swe::userClasses(model)) {
            const Scope* s = cls->getParentScope();
            if (!s || s->asSymbol().kind != SymbolKind::Package || generated.at(cls->location)) continue;
            const auto* pkg = &s->asSymbol().as<PackageSymbol>();
            size_t tokens = 0;
            std::string shape = classShape(*sm, *cls, tokens);
            if (tokens < minTokens) continue; // too small to tell a copy from a coincidence
            if (!classCount.count(pkg)) order.push_back(pkg);
            classCount[pkg]++;
            shapes[pkg].insert(shape);
        }
        for (size_t i = 0; i < order.size(); i++) {
            for (size_t j = 0; j < i; j++) {
                const auto* a = order[j];
                const auto* b = order[i];
                std::vector<std::string> common;
                std::set_intersection(shapes[a].begin(), shapes[a].end(), shapes[b].begin(), shapes[b].end(),
                                      std::back_inserter(common));
                size_t largest = std::max(classCount[a], classCount[b]);
                if (common.size() < minClasses || int64_t(common.size() * 100) < minPercent * int64_t(largest)) continue;
                diags.addViolation(
                    "SV-DUP-005", "CopiedAgentPackage",
                    "Package '" + std::string(b->name) + "' matches '" + std::string(a->name) + "' (" +
                        where(comp, a->location) + ") in " + std::to_string(common.size()) + " of " +
                        std::to_string(largest) + " classes, apart from names. It looks like a copied "
                        "agent; make one parameterized agent, or a shared base with small subclasses, "
                        "so a fix lands in both.",
                    core::Severity::Info, b->location);
                break;
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCloneFragments() { return std::make_unique<methodology::CloneFragments>(); }
std::unique_ptr<Rule> createDuplicatedConstraint() { return std::make_unique<methodology::DuplicatedConstraint>(); }
std::unique_ptr<Rule> createRepeatedStringLiteral() { return std::make_unique<methodology::RepeatedStringLiteral>(); }
std::unique_ptr<Rule> createCopiedAgentPackage() { return std::make_unique<methodology::CopiedAgentPackage>(); }
}
