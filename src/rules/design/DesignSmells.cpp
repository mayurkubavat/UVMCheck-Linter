#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Constraints.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include <algorithm>
#include <map>
#include <set>
#include <string>
#include <vector>

namespace uvmcheck::rules::design {

using namespace slang::ast;

// --- SV-DSN-012: a bit vector used as an enumeration ----------------------------------

class PrimitiveObsession : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-012"; }
    std::string_view getName() const override { return "PrimitiveObsession"; }
    std::string_view getDescription() const override {
        return "Flags a rand integral field of a sequence item or config that a constraint limits "
               "to a list of 3 to 16 single constant values ('x inside {0, 1, 2}'). The field is an "
               "enumeration: as an enum it prints by name in logs and waveforms and the compiler "
               "checks every value. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            const auto role = swe::classRole(*cls);
            if (role != swe::Role::Item && role != swe::Role::Config && role != swe::Role::Object) continue;
            std::set<const Symbol*> reported;
            for (const auto& cb : cls->membersOfType<ConstraintBlockSymbol>()) {
                walk(cb.getConstraints(), *cls, reported, diags);
            }
        }
    }

private:
    static void walk(const Constraint& c, const ClassType& cls, std::set<const Symbol*>& reported,
                     core::DiagnosticEngine& diags) {
        switch (c.kind) {
            case ConstraintKind::List:
                for (const auto* item : c.as<ConstraintList>().list) {
                    if (item) walk(*item, cls, reported, diags);
                }
                return;
            case ConstraintKind::Expression:
                check(c.as<ExpressionConstraint>().expr, cls, reported, diags);
                return;
            default:
                return;
        }
    }

    static void check(const Expression& expr, const ClassType& cls, std::set<const Symbol*>& reported,
                      core::DiagnosticEngine& diags) {
        const Expression& e = swe::stripConversions(expr);
        if (e.kind != ExpressionKind::Inside) return;
        const auto& in = e.as<InsideExpression>();
        const Symbol* field = swe::referencedSymbol(in.left());
        if (!field || field->kind != SymbolKind::ClassProperty) return;
        const auto& prop = field->as<ClassPropertySymbol>();
        if (prop.randMode == RandMode::None || &prop.getParentScope()->asSymbol() != &cls) return;
        const Type& t = prop.getType().getCanonicalType();
        if (!t.isIntegral() || t.isEnum() || t.getBitWidth() <= 1) return;
        const auto items = in.rangeList();
        if (items.size() < 3 || items.size() > 16) return;
        for (const auto* item : items) {
            if (!item || swe::stripConversions(*item).kind == ExpressionKind::ValueRange ||
                !swe::isConstantExpr(*item)) {
                return;
            }
        }
        if (!reported.insert(field).second) return;
        diags.addViolation(
            "SV-DSN-012", "PrimitiveObsession",
            "'" + std::string(prop.name) + "' only ever takes " + std::to_string(items.size()) +
                " listed values, so it is an enumeration written as a bit vector. Declare a typedef "
                "enum: logs and waveforms then show names instead of numbers, and a wrong value no "
                "longer compiles.",
            core::Severity::Info, prop.location);
    }
};

// --- SV-DSN-013: an override that only refuses to work -------------------------------

class FatalStubOverride : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-013"; }
    std::string_view getName() const override { return "FatalStubOverride"; }
    std::string_view getDescription() const override {
        return "Flags an override of a user base-class method whose whole body is an error or fatal "
               "report (\"not supported here\"). The subclass refuses part of its parent's contract, "
               "so code holding a base handle fails at run time (Fowler 'refused bequest'). Split the "
               "base class instead. SV-DSN-007 covers empty overrides.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->getParentScope() != cls) continue;
                const SubroutineSymbol* base = m->getOverride();
                if (!base || swe::isLibrarySymbol(*base) || base->flags.has(MethodFlags::Pure)) continue;
                const Statement& body = m->getBody();
                const auto level = swe::strongestReport(body);
                if (level != swe::ReportLevel::Error && level != swe::ReportLevel::Fatal) continue;
                if (!onlyReports(body)) continue;
                const Symbol* baseCls = base->getParentScope() ? &base->getParentScope()->asSymbol() : nullptr;
                diags.addViolation(
                    "SV-DSN-013", "FatalStubOverride",
                    "'" + std::string(cls->name) + "::" + std::string(m->name) + "' only reports an error, "
                        "refusing '" + (baseCls ? std::string(baseCls->name) + "::" : std::string()) +
                        std::string(base->name) + "'. Anything holding a base handle now fails at run "
                        "time. Move the shared part into a common base and give each variant only "
                        "what it supports.",
                    core::Severity::Warning, m->location);
            }
        }
    }

private:
    // True when every statement of the body is part of a report.
    static bool onlyReports(const Statement& body) {
        struct Checker : public ASTVisitor<Checker, VisitFlags::AllGood> {
            bool other = false;
            void handle(const ExpressionStatement& s) {
                if (s.expr.kind == ExpressionKind::Call) {
                    if (swe::reportLevel(s.expr.as<CallExpression>()) != swe::ReportLevel::None) return;
                    if (s.expr.as<CallExpression>().isSystemCall()) return; // $fatal/$error and friends
                }
                if (!swe::isReportScaffolding(s)) other = true;
            }
            void handle(const ConditionalStatement& c) {
                if (swe::isReportGuard(c)) return visitDefault(c);
                other = true;
            }
            void handle(const VariableDeclStatement& v) {
                if (!swe::isReportScaffolding(v)) other = true;
            }
            void handle(const ReturnStatement&) { other = true; }
            void handle(const TimedStatement&) { other = true; }
        } checker;
        body.visit(checker);
        return !checker.other;
    }
};

// --- SV-DED-015: class parameters no specialization ever sets -------------------------

class NeverSetClassParameter : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-015"; }
    std::string_view getName() const override { return "NeverSetClassParameter"; }
    std::string_view getDescription() const override {
        return "Flags a parameter of a user parameterized class that no specialization in the linted "
               "design ever sets. The class claims a flexibility nobody uses or verifies (Fowler "
               "'speculative generality'); make it a localparam or drop it. Whole-program: a "
               "specialization in a file that was not linted is not seen, and only specializations "
               "used as a class member or base class are counted. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        // The model lists plain classes; parameterized ones are found through the classes that
        // use a specialization of them (as a member type or a base class).
        std::vector<const GenericClassDefSymbol*> generics;
        std::set<const GenericClassDefSymbol*> seen;
        auto note = [&](const Type& t) {
            const auto* c = swe::asClass(t);
            if (!c || !c->genericClass || swe::isLibrarySymbol(*c->genericClass)) return;
            if (seen.insert(c->genericClass).second) generics.push_back(c->genericClass);
        };
        for (const auto* cls : model.getAllClasses()) {
            if (!cls || swe::isLibrarySymbol(*cls)) continue;
            if (const Type* base = cls->getBaseClass()) note(*base);
            for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) note(prop.getType());
        }
        for (const auto* gen : generics) {
            // name -> set by some specialization
            std::map<std::string_view, bool> set;
            std::map<std::string_view, const Symbol*> decl;
            for (const auto& spec : gen->specializations()) {
                if (!spec.isClass()) continue;
                for (const auto& member : spec.getCanonicalType().as<ClassType>().members()) {
                    if (member.kind == SymbolKind::Parameter) {
                        const auto& p = member.as<ParameterSymbol>();
                        if (p.isLocalParam() || !p.isPortParam()) continue;
                        set[p.name] = set[p.name] || p.isOverridden();
                        decl.emplace(p.name, &p);
                    } else if (member.kind == SymbolKind::TypeParameter) {
                        const auto& p = member.as<TypeParameterSymbol>();
                        if (p.isLocalParam() || !p.isPortParam()) continue;
                        set[p.name] = set[p.name] || p.isOverridden();
                        decl.emplace(p.name, &p);
                    }
                }
            }
            for (const auto& [name, used] : set) {
                if (used) continue;
                diags.addViolation(
                    "SV-DED-015", "NeverSetClassParameter",
                    "Parameter '" + std::string(name) + "' of class '" + std::string(gen->name) +
                        "' is never set by any specialization; every user takes the default. Make it a "
                        "localparam, or remove it until a second configuration exists and is tested.",
                    core::Severity::Info, decl[name]->location);
            }
        }
    }

};

// --- SV-DUP-006: the same argument group repeated across prototypes -------------------

class DataClumps : public Rule {
public:
    std::string_view getId() const override { return "SV-DUP-006"; }
    std::string_view getName() const override { return "DataClumps"; }
    std::string_view getDescription() const override {
        return "Flags the same group of min_args (default 4) or more arguments (same names and "
               "types) declared by min_routines (default 3) or more routines. Values that always "
               "travel together belong in a transaction or struct; passing them one by one lets "
               "two same-typed arguments be swapped silently (Fowler 'data clumps'). Overrides are "
               "counted once, at the base declaration; each group is reported once, at its first "
               "routine. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto minArgs = swe::ruleOption("SV-DUP-006", "min_args", 4);
        const auto minRoutines = swe::ruleOption("SV-DUP-006", "min_routines", 3);
        // Signature of the argument list (sorted "type name" pairs) -> routines declaring it.
        std::map<std::string, std::vector<const SubroutineSymbol*>> groups;
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->getParentScope() != cls || m->getOverride()) continue;
                const auto args = m->getArguments();
                if (static_cast<int64_t>(args.size()) < minArgs) continue;
                std::vector<std::string> parts;
                for (const auto* a : args) parts.push_back(a->getType().toString() + " " + std::string(a->name));
                std::sort(parts.begin(), parts.end());
                std::string key;
                for (const auto& p : parts) key += p + ";";
                groups[key].push_back(m);
            }
        }
        for (const auto& [key, routines] : groups) {
            if (static_cast<int64_t>(routines.size()) < minRoutines) continue;
            // One finding per group, at the first routine in the source, naming the others.
            std::vector<const SubroutineSymbol*> sorted(routines);
            std::sort(sorted.begin(), sorted.end(), [](const SubroutineSymbol* a, const SubroutineSymbol* b) {
                return a->location < b->location;
            });
            std::string names;
            for (size_t i = 1; i < sorted.size() && i <= 5; ++i) {
                const auto& owner = sorted[i]->getParentScope()->asSymbol();
                names += (names.empty() ? "" : ", ") + std::string(owner.name) + "::" + std::string(sorted[i]->name);
            }
            if (sorted.size() > 6) names += ", ...";
            const auto* first = sorted.front();
            diags.addViolation(
                "SV-DUP-006", "DataClumps",
                "'" + std::string(first->name) + "' and " + std::to_string(sorted.size() - 1) +
                    " other routine(s) (" + names + ") take the same " +
                    std::to_string(first->getArguments().size()) +
                    " arguments. Pass one transaction or struct that holds them.",
                core::Severity::Info, first->location);
        }
    }
};

// --- UVM-ROL-007: cycle-counting timeouts written procedurally in a monitor -----------

class ProceduralTimeoutInMonitor : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-007"; }
    std::string_view getName() const override { return "ProceduralTimeoutInMonitor"; }
    std::string_view getDescription() const override {
        return "Flags fork ... join_any blocks in a monitor where one branch counts clock cycles "
               "(repeat (N) @(...)) and then reports an error: a handshake timeout written as "
               "procedural code. A concurrent assertion in the interface states the same rule in "
               "one line and checks it every cycle. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Monitor) continue;
            Finder finder(*cls, diags);
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->getParentScope() == cls) m->getBody().visit(finder);
            }
        }
    }

private:
    struct Finder : public ASTVisitor<Finder, VisitFlags::AllGood> {
        Finder(const ClassType& cls, core::DiagnosticEngine& diags) : cls(cls), diags(diags) {}

        void handle(const BlockStatement& block) {
            if (block.blockKind == StatementBlockKind::JoinAny && hasCountingBranch(block.body)) {
                diags.addViolation(
                    "UVM-ROL-007", "ProceduralTimeoutInMonitor",
                    "Monitor '" + std::string(cls.name) + "' times a handshake by counting cycles in a "
                        "fork ... join_any. Write the rule as an assertion in the interface (req |-> "
                        "##[1:N] ack) and keep the monitor to collecting transactions.",
                    core::Severity::Info, block.sourceRange.start());
            }
            visitDefault(block);
        }

        // A branch holding repeat (...) with a clock wait and an error report after it.
        static bool hasCountingBranch(const Statement& body) {
            struct Branch : public ASTVisitor<Branch, VisitFlags::AllGood> {
                bool repeat = false;
                void handle(const RepeatLoopStatement& r) {
                    struct EdgeWait : public ASTVisitor<EdgeWait, VisitFlags::AllGood> {
                        bool found = false;
                        void handle(const TimedStatement& t) {
                            if (t.timing.kind == TimingControlKind::SignalEvent) found = true;
                            visitDefault(t);
                        }
                    } edge;
                    r.body.visit(edge);
                    repeat = repeat || edge.found;
                }
            } branch;
            body.visit(branch);
            const auto level = swe::strongestReport(body);
            return branch.repeat && (level == swe::ReportLevel::Error || level == swe::ReportLevel::Fatal);
        }

        const ClassType& cls;
        core::DiagnosticEngine& diags;
    };
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPrimitiveObsession() {
    return std::make_unique<design::PrimitiveObsession>();
}
std::unique_ptr<Rule> createFatalStubOverride() {
    return std::make_unique<design::FatalStubOverride>();
}
std::unique_ptr<Rule> createNeverSetClassParameter() {
    return std::make_unique<design::NeverSetClassParameter>();
}
std::unique_ptr<Rule> createDataClumps() {
    return std::make_unique<design::DataClumps>();
}
std::unique_ptr<Rule> createProceduralTimeoutInMonitor() {
    return std::make_unique<design::ProceduralTimeoutInMonitor>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createPrimitiveObsession)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createFatalStubOverride)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createNeverSetClassParameter)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDataClumps)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createProceduralTimeoutInMonitor)
