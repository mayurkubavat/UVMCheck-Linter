// Methodology guide chapter MG-2, config_db as coupling: UVM-CFG-007 to UVM-CFG-009.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/symbols/InstanceSymbols.h"

#include <algorithm>
#include <map>
#include <set>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

bool isConfigDbAccess(const CallExpression& call) {
    return meth::isConfigDbCall(call, "get") || meth::isConfigDbCall(call, "set") ||
           meth::isConfigDbCall(call, "exists") || meth::isConfigDbCall(call, "wait_modified");
}

bool isRunTimePhase(std::string_view m) {
    static const std::set<std::string_view> phases = {
        "run_phase",           "pre_reset_phase",    "reset_phase",     "post_reset_phase",
        "pre_configure_phase", "configure_phase",    "post_configure_phase",
        "pre_main_phase",      "main_phase",         "post_main_phase", "pre_shutdown_phase",
        "shutdown_phase",      "post_shutdown_phase"};
    return phases.count(m) > 0;
}

bool isBuildTime(std::string_view m) {
    return m == "new" || m == "build_phase" || m == "connect_phase" || m == "end_of_elaboration_phase" ||
           m == "start_of_simulation_phase";
}

const Expression& skipSelects(const Expression& e) {
    const Expression* x = &swe::stripConversions(e);
    while (true) {
        if (x->kind == ExpressionKind::ElementSelect) x = &swe::stripConversions(x->as<ElementSelectExpression>().value());
        else if (x->kind == ExpressionKind::RangeSelect) x = &swe::stripConversions(x->as<RangeSelectExpression>().value());
        else return *x;
    }
}

} // namespace

// UVM-CFG-007 -----------------------------------------------------------------------------

namespace {

class RuntimeConfigVisitor : public ASTVisitor<RuntimeConfigVisitor, VisitFlags::AllGood> {
public:
    RuntimeConfigVisitor(const ClassType& cls, const SubroutineSymbol& method, core::DiagnosticEngine& diags) :
        cls(cls), method(method), diags(diags),
        runTimePhase(swe::isComponentRole(swe::classRole(cls)) && isRunTimePhase(method.name)) {}

    void handle(const ForLoopStatement& s) { loop(s); }
    void handle(const RepeatLoopStatement& s) { loop(s); }
    void handle(const ForeachLoopStatement& s) { loop(s); }
    void handle(const WhileLoopStatement& s) { loop(s); }
    void handle(const DoWhileLoopStatement& s) { loop(s); }
    void handle(const ForeverLoopStatement& s) { loop(s); }

    void handle(const CallExpression& call) {
        if (isConfigDbAccess(call)) {
            bool inTaskLoop = loops > 0 && method.subroutineKind == SubroutineKind::Task;
            if (runTimePhase || inTaskLoop) {
                std::string where = std::string(cls.name) + "::" + std::string(method.name);
                std::string why = runTimePhase
                                      ? "during the run-time phases"
                                      : "inside a loop of a task";
                diags.addViolation(
                    "UVM-CFG-007", "ConfigDbAsRuntimeChannel",
                    "'" + where + "' calls uvm_config_db::" + std::string(call.getSubroutineName()) + "() " +
                        why + ". config_db is for wiring the testbench while it is built; used as a "
                        "run-time mailbox it is slow (a regex lookup per call) and hides who talks to "
                        "whom. Read the value once in build_phase, and send run-time changes through a "
                        "TLM port, an event or a method of the config object.",
                    core::Severity::Warning, swe::locOf(call));
            }
        }
        visitDefault(call);
    }

private:
    const ClassType& cls;
    const SubroutineSymbol& method;
    core::DiagnosticEngine& diags;
    bool runTimePhase;
    int loops = 0;

    template<typename T>
    void loop(const T& s) {
        loops++;
        visitDefault(s);
        loops--;
    }
};

} // namespace

class ConfigDbAsRuntimeChannel : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-007"; }
    std::string_view getName() const override { return "ConfigDbAsRuntimeChannel"; }
    std::string_view getDescription() const override {
        return "Flags uvm_config_db get, set, exists and wait_modified calls in the run-time phases "
               "of components and inside loops of any task. config_db is build-time wiring; used at "
               "run time it becomes a slow global mailbox (common coupling, MG-2.3).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                RuntimeConfigVisitor v(*cls, *m, diags);
                m->getBody().visit(v);
            }
        }
    }
};

// UVM-CFG-008 -----------------------------------------------------------------------------

namespace {

struct KeyUse {
    std::string file;
    size_t offset = 0;
    slang::SourceLocation loc;
};

class ConfigKeyCollector : public ASTVisitor<ConfigKeyCollector, VisitFlags::AllGood> {
public:
    ConfigKeyCollector(const slang::SourceManager& sm, std::map<std::string, std::vector<KeyUse>>& keys,
                       std::unordered_set<const CallExpression*>& seen) : sm(sm), keys(keys), seen(seen) {}

    void handle(const CallExpression& call) {
        if ((meth::isConfigDbCall(call, "set") || meth::isConfigDbCall(call, "get")) &&
            call.arguments().size() >= 3 && meth::isStringLiteral(*call.arguments()[2]) &&
            seen.insert(&call).second) {
            std::string key = meth::literalValue(*call.arguments()[2]);
            slang::SourceLocation loc = sm.getFullyOriginalLoc(swe::locOf(call));
            if (!key.empty() && key != "*" && meth::isUserLocation(sm, loc)) {
                keys[key].push_back({std::string(sm.getRawFileName(loc.buffer())), loc.offset(), loc});
            }
        }
        visitDefault(call);
    }

private:
    const slang::SourceManager& sm;
    std::map<std::string, std::vector<KeyUse>>& keys;
    std::unordered_set<const CallExpression*>& seen;
};

} // namespace

class ScatteredConfigKeyLiteral : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-008"; }
    std::string_view getName() const override { return "ScatteredConfigKeyLiteral"; }
    std::string_view getDescription() const override {
        return "Flags a uvm_config_db field name typed as a string literal in two or more files. "
               "The setter and getter only meet if both spell it the same; a shared constant makes "
               "the compiler check that (connascence of name, MG-2.4). Reported once per name.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        std::map<std::string, std::vector<KeyUse>> keys;
        std::unordered_set<const CallExpression*> seen;
        ConfigKeyCollector v(*sm, keys, seen);
        for (const auto* sub : swe::allUserRoutines(comp, model)) sub->getBody().visit(v);
        for (const auto* inst : comp.getRoot().topInstances) inst->body.visit(v);

        for (auto& [key, uses] : keys) {
            std::set<std::string> files;
            for (const auto& u : uses) files.insert(u.file);
            if (files.size() < 2) continue;
            std::sort(uses.begin(), uses.end(), [](const KeyUse& a, const KeyUse& b) {
                return std::tie(a.file, a.offset) < std::tie(b.file, b.offset);
            });
            std::string others;
            size_t n = 0;
            for (const auto& f : files) {
                if (f == uses.front().file) continue;
                if (n++ == 3) { others += ", ..."; break; }
                std::string_view base = f;
                if (size_t slash = base.find_last_of("/\\"); slash != std::string_view::npos) base.remove_prefix(slash + 1);
                others += (others.empty() ? "" : ", ") + std::string(base);
            }
            diags.addViolation(
                "UVM-CFG-008", "ScatteredConfigKeyLiteral",
                "The config_db field name \"" + key + "\" is typed as a literal " + std::to_string(uses.size()) +
                    " times in " + std::to_string(files.size()) + " files (also in " + others + "). If one "
                    "spelling changes, get() quietly stops finding the value. Declare it once, for "
                    "example as a localparam string in the agent's package, and use that name.",
                core::Severity::Warning, uses.front().loc);
        }
    }
};

// UVM-CFG-009 -----------------------------------------------------------------------------

namespace {

/// Symbols a class fills from config_db get(), and the ones it creates itself.
class ConfigHandleScanner : public ASTVisitor<ConfigHandleScanner, VisitFlags::AllGood> {
public:
    std::unordered_set<const Symbol*> fetched, created;

    void handle(const CallExpression& call) {
        if (meth::isConfigDbCall(call, "get") && call.arguments().size() >= 4) {
            const Expression& target = swe::stripConversions(*call.arguments()[3]);
            // ref arguments are wrapped in an assignment to themselves.
            const Expression* t = &target;
            if (t->kind == ExpressionKind::Assignment) t = &swe::stripConversions(t->as<AssignmentExpression>().left());
            if (const Symbol* s = swe::referencedSymbol(*t); s && t->type && meth::classOfType(*t->type)) fetched.insert(s);
        }
        visitDefault(call);
    }

    void handle(const AssignmentExpression& a) {
        const Expression& rhs = swe::stripConversions(a.right());
        bool creates = rhs.kind == ExpressionKind::NewClass ||
                       (rhs.kind == ExpressionKind::Call && meth::isFactoryCreate(rhs.as<CallExpression>()));
        if (creates) {
            if (const Symbol* s = swe::referencedSymbol(skipSelects(a.left()))) created.insert(s);
        }
        visitDefault(a);
    }
};

class ConfigWriteVisitor : public ASTVisitor<ConfigWriteVisitor, VisitFlags::AllGood> {
public:
    ConfigWriteVisitor(const ClassType& cls, const SubroutineSymbol& method,
                       const std::unordered_set<const Symbol*>& shared, core::DiagnosticEngine& diags) :
        cls(cls), method(method), shared(shared), diags(diags) {}

    void handle(const AssignmentExpression& a) {
        const Expression& lhs = skipSelects(a.left());
        // Output and ref arguments are bound as assignments from an empty argument; a
        // config_db get() into a field is wiring, not a run-time update.
        if (a.right().kind == ExpressionKind::EmptyArgument) {
            visitDefault(a);
            return;
        }
        if (lhs.kind == ExpressionKind::MemberAccess &&
            lhs.as<MemberAccessExpression>().member.kind == SymbolKind::ClassProperty) {
            const Symbol* root = swe::rootSymbol(lhs);
            if (root && shared.count(root)) {
                diags.addViolation(
                    "UVM-CFG-009", "SharedConfigMutatedAtRuntime",
                    "'" + std::string(cls.name) + "::" + std::string(method.name) + "' writes '" +
                        swe::exprText(lhs) + "' in a config object it got from uvm_config_db. Every "
                        "component that read the same object sees the change at an unknown time. Let "
                        "the owner that created the object change it, or copy it before changing it.",
                    core::Severity::Warning, swe::locOf(lhs));
            }
        }
        visitDefault(a);
    }

private:
    const ClassType& cls;
    const SubroutineSymbol& method;
    const std::unordered_set<const Symbol*>& shared;
    core::DiagnosticEngine& diags;
};

/// Methods of the class that are only called from new() and the build phases (directly or
/// through other such helpers): they run while the testbench is built.
std::unordered_set<const SubroutineSymbol*> buildTimeHelpers(const std::vector<const SubroutineSymbol*>& methods) {
    std::unordered_map<const SubroutineSymbol*, std::unordered_set<const SubroutineSymbol*>> callers;
    std::unordered_set<const SubroutineSymbol*> own(methods.begin(), methods.end());
    for (const auto* m : methods) {
        swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
            const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
            if (!call.isSystemCall() && sub && *sub && own.count(*sub)) callers[*sub].insert(m);
        });
    }
    std::unordered_set<const SubroutineSymbol*> out;
    for (bool changed = true; changed;) {
        changed = false;
        for (const auto* m : methods) {
            if (out.count(m) || isBuildTime(m->name) || !callers.count(m)) continue;
            bool all = std::all_of(callers[m].begin(), callers[m].end(), [&](const SubroutineSymbol* c) {
                return isBuildTime(c->name) || out.count(c);
            });
            if (all) {
                out.insert(m);
                changed = true;
            }
        }
    }
    return out;
}

} // namespace

class SharedConfigMutatedAtRuntime : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-009"; }
    std::string_view getName() const override { return "SharedConfigMutatedAtRuntime"; }
    std::string_view getDescription() const override {
        return "Flags writes to fields of a config object that the class got from uvm_config_db and "
               "did not create, made outside new() and the build phases. The object is shared by "
               "everyone who read it, so the write is hidden global state (common coupling, MG-2.3).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            auto methods = swe::classMethods(*cls);
            ConfigHandleScanner scan;
            for (const auto* m : methods) m->getBody().visit(scan);
            std::unordered_set<const Symbol*> shared;
            for (const auto* s : scan.fetched) {
                if (!scan.created.count(s)) shared.insert(s);
            }
            if (shared.empty()) continue;
            auto buildOnly = buildTimeHelpers(methods);
            for (const auto* m : methods) {
                if (isBuildTime(m->name) || buildOnly.count(m)) continue;
                ConfigWriteVisitor v(*cls, *m, shared, diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConfigDbAsRuntimeChannel() { return std::make_unique<methodology::ConfigDbAsRuntimeChannel>(); }
std::unique_ptr<Rule> createScatteredConfigKeyLiteral() { return std::make_unique<methodology::ScatteredConfigKeyLiteral>(); }
std::unique_ptr<Rule> createSharedConfigMutatedAtRuntime() { return std::make_unique<methodology::SharedConfigMutatedAtRuntime>(); }
}
