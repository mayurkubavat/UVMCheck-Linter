#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::performance {

class VerbosityFormatVisitor : public slang::ast::ASTVisitor<VerbosityFormatVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit VerbosityFormatVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ConditionalStatement& cond) {
        bool isReportGuard = false;
        for (const auto& c : cond.conditions) {
            if (c.expr->syntax) {
                std::string condText = std::string(c.expr->syntax->toString());
                if (condText.find("uvm_report_enabled") != std::string::npos) {
                    isReportGuard = true;
                    break;
                }
            }
        }

        if (isReportGuard) reportGuardDepth++;
        visitDefault(cond);
        if (isReportGuard) reportGuardDepth--;
    }

    void handle(const slang::ast::CallExpression& call) {
        std::string subName = std::string(call.getSubroutineName());
        bool isInfoOrWarning = (subName.find("uvm_report_info") != std::string::npos ||
                                subName.find("uvm_info") != std::string::npos ||
                                subName.find("uvm_report_warning") != std::string::npos ||
                                subName.find("uvm_warning") != std::string::npos);

        if (isInfoOrWarning && reportGuardDepth == 0) {
            auto args = call.arguments();
            if (args.size() >= 2) {
                const auto* msgArg = args[1];
                bool hasSformatf = false;
                if (msgArg->syntax) {
                    std::string msgText = std::string(msgArg->syntax->toString());
                    if (msgText.find("$sformatf") != std::string::npos) {
                        hasSformatf = true;
                    }
                }

                bool isHighVerbosity = false;
                if (args.size() >= 3) {
                    const auto* verbArg = args[2];
                    if (verbArg->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                        if (verbArg->as<slang::ast::IntegerLiteral>().getValue() > 200) {
                            isHighVerbosity = true;
                        }
                    } else if (verbArg->syntax) {
                        std::string verbText = std::string(verbArg->syntax->toString());
                        if (verbText.find("UVM_HIGH") != std::string::npos ||
                            verbText.find("UVM_FULL") != std::string::npos ||
                            verbText.find("UVM_DEBUG") != std::string::npos ||
                            verbText == "300" || verbText == "400" || verbText == "500") {
                            isHighVerbosity = true;
                        }
                    }
                }

                if (hasSformatf && isHighVerbosity) {
                    diags.addViolation(
                        "SV-PRF-005",
                        "UnshieldedVerbosityFormatEval",
                        "Calling $sformatf inside uvm_info/warning with high verbosity without checking 'uvm_report_enabled'. "
                        "Because macro/subroutine arguments evaluate eagerly, expensive string formatting and heap allocations execute "
                        "on every transaction even when messages are suppressed by simulation verbosity. "
                        "Wrap call in 'if (uvm_report_enabled(...))' or use `uvm_info_begin/end.",
                        core::Severity::Warning,
                        msgArg->sourceRange.start()
                    );
                }
            }
        }
        visitDefault(call);
    }

private:
    int reportGuardDepth = 0;
    core::DiagnosticEngine& diags;
};

class UnshieldedVerbosityFormatEval : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-005"; }
    std::string_view getName() const override { return "UnshieldedVerbosityFormatEval"; }
    std::string_view getDescription() const override {
        return "Calling $sformatf inside uvm_info/warning without checking uvm_report_enabled formats strings even when messages are discarded by verbosity.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        VerbosityFormatVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnshieldedVerbosityFormatEval() {
    return std::make_unique<performance::UnshieldedVerbosityFormatEval>();
}
}
