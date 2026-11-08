#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::uvm {

class DisplayCallVisitor : public slang::ast::ASTVisitor<DisplayCallVisitor, slang::ast::VisitFlags::AllGood> {
public:
    DisplayCallVisitor(core::DiagnosticEngine& diags, std::string_view className, const slang::SourceManager* sm)
        : diags(diags), className(className), sm(sm) {}

    /// Instance methods only: the fix calls get_type_name().
    bool inInstanceMethod = true;

    // `$display("fmt", args);` as a whole statement becomes `uvm_info(get_type_name(), $sformatf("fmt", args), UVM_LOW).
    void handle(const slang::ast::ExpressionStatement& stmt) {
        using namespace slang::ast;
        if (sm && inInstanceMethod && stmt.expr.kind == ExpressionKind::Call && stmt.syntax) {
            const auto& call = stmt.expr.as<CallExpression>();
            const auto args = call.arguments();
            const auto range = stmt.syntax->sourceRange();
            if (call.getSubroutineName() == "$display" && !args.empty() &&
                args[0]->kind == ExpressionKind::StringLiteral && range.start().valid() &&
                !sm->isMacroLoc(range.start()) && !sm->isMacroLoc(range.end()) && call.syntax) {
                std::string text(call.syntax->toString());
                const size_t open = text.find('(');
                const size_t close = text.rfind(')');
                if (open != std::string::npos && close != std::string::npos && close > open) {
                    pendingFix = core::FixItHint{
                        range, "`uvm_info(get_type_name(), $sformatf" + text.substr(open, close - open + 1) + ", UVM_LOW)",
                        "Report through `uvm_info", core::FixSafety::Unsafe};
                    pendingCall = &call;
                }
            }
        }
        visitDefault(stmt);
        pendingCall = nullptr;
    }

    void handle(const slang::ast::CallExpression& expr) {
        std::string_view subName = expr.getSubroutineName();
        if (subName == "$display" || subName == "$displayh" || subName == "$displayb" || subName == "$displayo" ||
            subName == "$write" || subName == "$writeh" || subName == "$writeb" || subName == "$writeo" ||
            subName == "$strobe" || subName == "$strobeh" || subName == "$strobeb" || subName == "$strobeo") {

            diags.addViolation(
                "UVM-LOG-002",
                "ProhibitedDisplayInUvm",
                "Direct system task '" + std::string(subName) + "' called in UVM class '" + std::string(className) +
                "'. Direct display tasks bypass UVM report catchers, severity filtering (+uvm_set_verbosity), and regression log routing. Use `uvm_info or `uvm_error instead.",
                core::Severity::Warning,
                expr.sourceRange.start(),
                // Unsafe: the message now obeys verbosity (UVM_LOW) and report catchers.
                pendingCall == &expr ? std::vector<core::FixItHint>{pendingFix} : std::vector<core::FixItHint>{}
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view className;
    const slang::SourceManager* sm;
    const slang::ast::CallExpression* pendingCall = nullptr;
    core::FixItHint pendingFix;
};

class ProhibitedDisplayInUvm : public Rule {
public:
    std::string_view getId() const override { return "UVM-LOG-002"; }
    std::string_view getName() const override { return "ProhibitedDisplayInUvm"; }
    std::string_view getDescription() const override {
        return "Disallow direct $display/$write/$strobe calls inside UVM classes (components, objects, sequences). "
               "SystemVerilog display tasks bypass UVM reporting infrastructure, log redirection, and verbosity filtering.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            if (!classType) continue;
            if (!uvmModel.isDerivedFrom(*classType, "uvm_report_object") &&
                !uvmModel.isUvmComponent(*classType) &&
                !uvmModel.isUvmSequence(*classType) &&
                !uvmModel.isDerivedFrom(*classType, "uvm_object")) {
                continue;
            }

            DisplayCallVisitor visitor(diags, classType->name, compilation.getSourceManager());
            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                visitor.inInstanceMethod = !(sub.flags & slang::ast::MethodFlags::Static);
                sub.getBody().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProhibitedDisplayInUvm() {
    return std::make_unique<uvm::ProhibitedDisplayInUvm>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createProhibitedDisplayInUvm)
