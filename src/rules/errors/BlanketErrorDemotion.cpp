#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::errors {

using namespace slang::ast;

namespace {

bool mentions(const std::string& text, std::string_view word) {
    return text.find(word) != std::string::npos;
}

bool isErrorSeverity(const std::string& arg) {
    return mentions(arg, "UVM_ERROR") || mentions(arg, "UVM_FATAL");
}

bool isWeakSeverity(const std::string& arg) {
    return mentions(arg, "UVM_INFO") || mentions(arg, "UVM_WARNING");
}

// An action that neither counts the error nor stops the run.
bool dropsAction(const std::string& arg) {
    return mentions(arg, "UVM_NO_ACTION") || (!mentions(arg, "UVM_COUNT") && !mentions(arg, "UVM_EXIT") &&
                                              !mentions(arg, "UVM_STOP"));
}

struct ReturnFinder : public ASTVisitor<ReturnFinder, VisitFlags::AllGood> {
    bool returnsCaught = false;
    void handle(const ReturnStatement& r) {
        if (r.expr && swe::exprText(*r.expr) == "CAUGHT") returnsCaught = true;
        visitDefault(r);
    }
};

} // namespace

class BlanketErrorDemotion : public Rule {
public:
    std::string_view getId() const override { return "UVM-REP-006"; }
    std::string_view getName() const override { return "BlanketErrorDemotion"; }
    std::string_view getDescription() const override {
        return "Flags report catchers that demote or swallow errors without filtering on ID or message, "
               "and severity overrides/actions that disable UVM_ERROR or UVM_FATAL for every ID "
               "(Pylint W0718 broad-exception-caught).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            bool catcher = swe::classRole(*cls) == swe::Role::Catcher;
            for (const auto* m : swe::classMethods(*cls)) {
                if (catcher && m->name == "catch") checkCatch(*cls, *m, diags);
                checkOverrides(*m, diags);
            }
        }
    }

private:
    static void checkCatch(const ClassType& cls, const SubroutineSymbol& m, core::DiagnosticEngine& diags) {
        bool filters = false;
        const CallExpression* demotion = nullptr;
        swe::forEachCall(m.getBody(), [&](const CallExpression& call) {
            std::string_view n = call.getSubroutineName();
            if (n == "get_id" || n == "get_message" || n == "get_fname" || n == "get_context" ||
                n == "get_client") {
                filters = true;
            }
            if (call.arguments().empty() || demotion) return;
            std::string arg = swe::exprText(*call.arguments()[0]);
            if ((n == "set_severity" && isWeakSeverity(arg)) || (n == "set_action" && dropsAction(arg)))
                demotion = &call;
        });
        ReturnFinder rf;
        m.getBody().visit(rf);
        if (filters || (!demotion && !rf.returnsCaught)) return;
        diags.addViolation(
            "UVM-REP-006", "BlanketErrorDemotion",
            "Report catcher '" + std::string(cls.name) + "' " +
                (demotion ? "demotes reports" : "returns CAUGHT") +
                " without checking get_id() or get_message(), so every matching error in the run is "
                "hidden, including ones nobody has seen yet. Filter on the specific ID or message.",
            core::Severity::Error, demotion ? swe::locOf(*demotion) : m.location);
    }

    static void checkOverrides(const SubroutineSymbol& m, core::DiagnosticEngine& diags) {
        swe::forEachCall(m.getBody(), [&](const CallExpression& call) {
            std::string_view n = call.getSubroutineName();
            auto args = call.arguments();
            if (args.size() < 2) return;
            std::string a0 = swe::exprText(*args[0]);
            std::string a1 = swe::exprText(*args[1]);
            bool bad = false;
            if (n == "set_report_severity_override" && isErrorSeverity(a0) && isWeakSeverity(a1)) bad = true;
            if ((n == "set_report_severity_action" || n == "set_report_severity_action_hier") &&
                isErrorSeverity(a0) && dropsAction(a1)) {
                bad = true;
            }
            if (!bad) return;
            diags.addViolation(
                "UVM-REP-006", "BlanketErrorDemotion",
                std::string(n) + "(" + a0 + ", " + a1 + ") changes how every " + a0 +
                    " is handled, for all report IDs. Errors stop counting and the test can pass "
                    "with real failures. Use the *_id_* variant for the specific ID instead.",
                core::Severity::Error, swe::locOf(call));
        });
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBlanketErrorDemotion() {
    return std::make_unique<errors::BlanketErrorDemotion>();
}
}
