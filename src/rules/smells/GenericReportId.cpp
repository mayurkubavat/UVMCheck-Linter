#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include <unordered_set>
#include <algorithm>

namespace uvmcheck::rules::smells {

class GenericReportIdVisitor : public slang::ast::ASTVisitor<GenericReportIdVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit GenericReportIdVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        std::string subName = std::string(call.getSubroutineName());
        bool isReportCall = (subName.find("uvm_report_info") != std::string::npos ||
                             subName.find("uvm_report_warning") != std::string::npos ||
                             subName.find("uvm_report_error") != std::string::npos ||
                             subName.find("uvm_report_fatal") != std::string::npos ||
                             subName.find("uvm_info") != std::string::npos ||
                             subName.find("uvm_error") != std::string::npos ||
                             subName.find("uvm_warning") != std::string::npos ||
                             subName.find("uvm_fatal") != std::string::npos);

        if (isReportCall) {
            auto args = call.arguments();
            if (!args.empty()) {
                const auto* idArg = args[0];
                while (idArg->kind == slang::ast::ExpressionKind::Conversion) {
                    idArg = &idArg->as<slang::ast::ConversionExpression>().operand();
                }

                if (idArg->kind == slang::ast::ExpressionKind::StringLiteral) {
                    std::string idVal = std::string(idArg->as<slang::ast::StringLiteral>().getValue());
                    std::string upperVal = idVal;
                    std::transform(upperVal.begin(), upperVal.end(), upperVal.begin(), ::toupper);

                    static const std::unordered_set<std::string_view> genericIds = {
                        "ERR", "ERROR", "INFO", "WARN", "WARNING", "DEBUG", "FATAL", "TEST", "TB", "MSG", "TEMP", ""
                    };

                    if (genericIds.count(upperVal) > 0) {
                        diags.addViolation(
                            "SV-SML-003",
                            "GenericReportId",
                            "Generic or cryptic UVM report ID '" + idVal +
                            "'. Using generic single-word report IDs prevents targeted log filtering and severity overrides via '+uvm_set_severity' and 'uvm_report_catcher'. "
                            "Use descriptive, feature-scoped identifiers (e.g. 'AXI_SLV_TIMEOUT', 'PCIE_TLP_CRC_ERR').",
                            core::Severity::Info,
                            idArg->sourceRange.start()
                        );
                    }
                }
            }
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
};

class GenericReportId : public Rule {
public:
    std::string_view getId() const override { return "SV-SML-003"; }
    std::string_view getName() const override { return "GenericReportId"; }
    std::string_view getDescription() const override {
        return "Disallow generic single-word report IDs ('ERR', 'INFO', 'DEBUG') in UVM reporting macros. Use feature-scoped hierarchical identifiers to support precise log filtering.";
    }
    std::string_view getCategory() const override { return "Smell"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        GenericReportIdVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::smells

namespace uvmcheck::rules {
std::unique_ptr<Rule> createGenericReportId() {
    return std::make_unique<smells::GenericReportId>();
}
}
