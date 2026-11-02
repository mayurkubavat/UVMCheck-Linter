#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Constraints.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include <map>
#include <set>
#include <string>
#include <vector>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

// --- UVM-PHS-011: run_phase next to runtime sub-phases ------------------------------

bool isRuntimePhase(std::string_view name) {
    static const std::set<std::string_view> phases = {
        "pre_reset_phase",     "reset_phase",     "post_reset_phase",
        "pre_configure_phase", "configure_phase", "post_configure_phase",
        "pre_main_phase",      "main_phase",      "post_main_phase",
        "pre_shutdown_phase",  "shutdown_phase",  "post_shutdown_phase"};
    return phases.count(name) != 0;
}

class RunPhaseWithRuntimePhases : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-011"; }
    std::string_view getName() const override { return "RunPhaseWithRuntimePhases"; }
    std::string_view getDescription() const override {
        return "Flags a component that overrides run_phase and also one of the runtime sub-phases "
               "(reset, configure, main, shutdown and their pre_/post_ phases). run_phase runs in "
               "parallel with all of them, so the two schedules race and readers cannot tell the order.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (!swe::isComponentRole(swe::classRole(*cls))) continue;
            const SubroutineSymbol* run = nullptr;
            std::vector<const SubroutineSymbol*> runtime;
            for (const auto& sub : cls->membersOfType<SubroutineSymbol>()) {
                if (sub.name == "run_phase") run = &sub;
                else if (isRuntimePhase(sub.name)) runtime.push_back(&sub);
            }
            if (!run || runtime.empty()) continue;
            std::string names;
            for (const auto* s : runtime) names += (names.empty() ? "" : ", ") + std::string(s->name);
            diags.addViolation(
                "UVM-PHS-011", "RunPhaseWithRuntimePhases",
                "'" + std::string(cls->name) + "' overrides run_phase and " + names +
                    ". run_phase runs alongside every runtime phase, so its stimulus or checks race "
                    "the reset/configure/main schedule. Use either run_phase or the runtime phases.",
                core::Severity::Warning, run->location);
        }
    }
};

// --- SV-RND-027: soft constraints that shadow each other ----------------------------

// Rand class properties named in an expression.
std::set<const Symbol*> randVars(const Expression& expr) {
    std::set<const Symbol*> out;
    for (const auto* s : swe::referencedSymbols(expr)) {
        if (s->kind == SymbolKind::ClassProperty && s->as<ClassPropertySymbol>().randMode != RandMode::None) {
            out.insert(s);
        }
    }
    return out;
}

template<typename F>
void forEachSoft(const Constraint& c, F&& f) {
    switch (c.kind) {
        case ConstraintKind::Expression: {
            const auto& e = c.as<ExpressionConstraint>();
            if (e.isSoft) f(e);
            break;
        }
        case ConstraintKind::List:
            for (const auto* item : c.as<ConstraintList>().list) {
                if (item) forEachSoft(*item, f);
            }
            break;
        case ConstraintKind::Implication:
            forEachSoft(c.as<ImplicationConstraint>().body, f);
            break;
        case ConstraintKind::Conditional: {
            const auto& cond = c.as<ConditionalConstraint>();
            forEachSoft(cond.ifBody, f);
            if (cond.elseBody) forEachSoft(*cond.elseBody, f);
            break;
        }
        case ConstraintKind::Foreach:
            forEachSoft(c.as<ForeachConstraint>().body, f);
            break;
        default:
            break;
    }
}

class InlineSoftVisitor : public swe::UserCodeVisitor<InlineSoftVisitor> {
public:
    using UserCodeVisitor::handle;
    explicit InlineSoftVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const CallExpression& call) {
        if (call.isSystemCall()) {
            const auto& info = std::get<1>(call.subroutine);
            if (const auto* r = std::get_if<CallExpression::RandomizeCallInfo>(&info.extraInfo);
                r && r->inlineConstraints) {
                forEachSoft(*r->inlineConstraints, [&](const ExpressionConstraint& e) {
                    diags.addViolation(
                        "SV-RND-027", "SoftConstraintShadowing",
                        "'soft' in an inline randomize() with { } block. The caller's intent is a "
                        "requirement here; a soft inline constraint quietly loses to any class "
                        "constraint, so readers cannot tell what value they will get. Drop 'soft'.",
                        core::Severity::Warning, e.expr.sourceRange.start());
                });
            }
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
};

class SoftConstraintShadowing : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-027"; }
    std::string_view getName() const override { return "SoftConstraintShadowing"; }
    std::string_view getDescription() const override {
        return "Flags soft constraints whose winner is hard to predict: 'soft' inside an inline "
               "randomize() with { } block, and a soft constraint on a variable that a base class "
               "or another constraint block of the same class already constrains softly. Keep soft "
               "defaults in one place, in the base class, and override with hard constraints.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        for (const auto* cls : swe::userClasses(ctx.uvmModel())) checkClass(*cls, diags);
        InlineSoftVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }

private:
    // Soft-constrained variables of a class's own constraint blocks, with the block naming them.
    static std::map<const Symbol*, const ConstraintBlockSymbol*> softTargets(const ClassType& cls) {
        std::map<const Symbol*, const ConstraintBlockSymbol*> out;
        for (const auto& cb : cls.membersOfType<ConstraintBlockSymbol>()) {
            forEachSoft(cb.getConstraints(), [&](const ExpressionConstraint& e) {
                for (const auto* v : randVars(e.expr)) out.emplace(v, &cb);
            });
        }
        return out;
    }

    static void checkClass(const ClassType& cls, core::DiagnosticEngine& diags) {
        // Soft targets of every user base class, nearest first.
        std::map<const Symbol*, std::string> inherited;
        const Type* base = cls.getBaseClass();
        for (int guard = 0; base && base->isClass() && guard < 64; ++guard) {
            const auto& bc = base->getCanonicalType().as<ClassType>();
            if (swe::isLibrarySymbol(bc)) break;
            for (const auto& [v, cb] : softTargets(bc)) inherited.emplace(v, std::string(bc.name));
            base = bc.getBaseClass();
        }

        std::map<const Symbol*, const ConstraintBlockSymbol*> seen;
        for (const auto& cb : cls.membersOfType<ConstraintBlockSymbol>()) {
            forEachSoft(cb.getConstraints(), [&](const ExpressionConstraint& e) {
                for (const auto* v : randVars(e.expr)) {
                    std::string other;
                    if (auto it = inherited.find(v); it != inherited.end()) {
                        other = "base class '" + it->second + "'";
                    } else if (auto s = seen.find(v); s != seen.end() && s->second != &cb) {
                        other = "constraint '" + std::string(s->second->name) + "'";
                    }
                    seen.emplace(v, &cb);
                    if (other.empty()) continue;
                    diags.addViolation(
                        "SV-RND-027", "SoftConstraintShadowing",
                        "'" + std::string(v->name) + "' is also constrained softly in " + other +
                            ". Soft constraints resolve by declaration order, so which default wins "
                            "is hidden from the reader. Keep one soft default and override it with a "
                            "hard constraint.",
                        core::Severity::Warning, e.expr.sourceRange.start());
                    return;
                }
            });
        }
    }
};

// --- UVM-ROL-006: virtual interface in a scoreboard or subscriber -------------------

class VirtualInterfaceInChecker : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-006"; }
    std::string_view getName() const override { return "VirtualInterfaceInChecker"; }
    std::string_view getDescription() const override {
        return "Flags virtual interface handles in scoreboards and subscribers. Pin timing belongs in "
               "drivers, monitors and interface assertions; a checker that waits on signals cannot "
               "run against a transaction-level model or an emulator, and hides a second monitor. "
               "Sequences are covered by UVM-SEQ-001.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            const auto role = swe::classRole(*cls);
            if (role != swe::Role::Scoreboard && role != swe::Role::Subscriber) continue;
            for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) {
                if (!prop.getType().isVirtualInterface()) continue;
                diags.addViolation(
                    "UVM-ROL-006", "VirtualInterfaceInChecker",
                    std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                        "' holds virtual interface '" + std::string(prop.name) +
                        "'. Have a monitor publish what the pins show (reset included) as "
                        "transactions, and check those.",
                    core::Severity::Warning, prop.location);
            }
        }
    }
};

// --- UVM-SEQ-016: sequencer handles fetched from config_db inside a sequence --------

bool isConfigDbGet(const CallExpression& call) {
    if (call.getSubroutineName() != "get" || call.isSystemCall()) return false;
    const auto* sub = std::get<0>(call.subroutine);
    if (!sub || !sub->getParentScope()) return false;
    const auto& owner = sub->getParentScope()->asSymbol();
    std::string_view name = owner.name;
    if (owner.kind == SymbolKind::ClassType) {
        if (const auto* gen = owner.as<ClassType>().genericClass) name = gen->name;
    }
    return name == "uvm_config_db" || name == "uvm_resource_db";
}

class SequencerConfigDbWalker : public ASTVisitor<SequencerConfigDbWalker, VisitFlags::AllGood> {
public:
    SequencerConfigDbWalker(const ClassType& seq, core::DiagnosticEngine& diags) : seq(seq), diags(diags) {}

    void handle(const CallExpression& call) {
        if (isConfigDbGet(call)) {
            const auto args = call.arguments();
            if (args.size() >= 4 && args[3] && args[3]->type) {
                if (const auto* cls = swe::asClass(*args[3]->type);
                    cls && swe::classRole(*cls) == swe::Role::Sequencer) {
                    diags.addViolation(
                        "UVM-SEQ-016", "SequencerFromConfigDb",
                        "Sequence '" + std::string(seq.name) + "' looks up sequencer '" +
                            std::string(cls->name) + "' in uvm_config_db. Every sequence that does "
                            "this repeats the plumbing and ties stimulus to hierarchy strings. Run it "
                            "on a virtual sequencer (p_sequencer) or have the test hand it the handles.",
                        core::Severity::Warning, call.sourceRange.start());
                }
            }
        }
        visitDefault(call);
    }

private:
    const ClassType& seq;
    core::DiagnosticEngine& diags;
};

class SequencerFromConfigDb : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-016"; }
    std::string_view getName() const override { return "SequencerFromConfigDb"; }
    std::string_view getDescription() const override {
        return "Flags uvm_config_db/uvm_resource_db get() calls in a sequence that fetch a "
               "sequencer handle. Bind sub-sequencers once, in a virtual sequencer or the test.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Sequence) continue;
            SequencerConfigDbWalker walker(*cls, diags);
            for (const auto* method : swe::classMethods(*cls)) {
                if (method->getParentScope() == cls) method->getBody().visit(walker);
            }
        }
    }
};

// --- SV-MET-009: configuration objects with too many knobs --------------------------

class ConfigKnobBudget : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-009"; }
    std::string_view getName() const override { return "ConfigKnobBudget"; }
    std::string_view getDescription() const override {
        return "Flags a configuration object (a uvm_object named *cfg* or *config*) that declares "
               "more than max_knobs (default 12) data members of its own. Split it into per-agent or "
               "per-feature sub-configs that the parent config holds. Advisory: SV-MET-006 already "
               "allows config objects up to 80 fields, so this stricter budget is opt-in.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-009", "max_knobs", 12);
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Config) continue;
            int64_t knobs = 0;
            for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) {
                if (prop.lifetime == VariableLifetime::Static || prop.flags.has(VariableFlags::Const)) continue;
                ++knobs;
            }
            if (knobs <= limit) continue;
            diags.addViolation(
                "SV-MET-009", "ConfigKnobBudget",
                "Config '" + std::string(cls->name) + "' has " + std::to_string(knobs) +
                    " knobs (budget " + std::to_string(limit) +
                    "). Group related knobs into sub-config objects so each agent or feature "
                    "reads only its own.",
                core::Severity::Warning, cls->location);
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRunPhaseWithRuntimePhases() {
    return std::make_unique<methodology::RunPhaseWithRuntimePhases>();
}
std::unique_ptr<Rule> createSoftConstraintShadowing() {
    return std::make_unique<methodology::SoftConstraintShadowing>();
}
std::unique_ptr<Rule> createVirtualInterfaceInChecker() {
    return std::make_unique<methodology::VirtualInterfaceInChecker>();
}
std::unique_ptr<Rule> createSequencerFromConfigDb() {
    return std::make_unique<methodology::SequencerFromConfigDb>();
}
std::unique_ptr<Rule> createConfigKnobBudget() {
    return std::make_unique<methodology::ConfigKnobBudget>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRunPhaseWithRuntimePhases)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSoftConstraintShadowing)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createVirtualInterfaceInChecker)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSequencerFromConfigDb)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createConfigKnobBudget)
