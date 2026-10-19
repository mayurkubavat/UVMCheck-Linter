#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include <string>
#include <algorithm>

namespace uvmcheck::rules::uvm {

static std::string toUpper(std::string_view s) {
    std::string res;
    res.reserve(s.size());
    for (char c : s) {
        res.push_back(static_cast<char>(std::toupper(static_cast<unsigned char>(c))));
    }
    return res;
}

class LowVerbosityDebugVisitor : public slang::ast::ASTVisitor<LowVerbosityDebugVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit LowVerbosityDebugVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        std::string subName = std::string(call.getSubroutineName());
        bool isInfoReport = (subName.find("uvm_report_info") != std::string::npos ||
                             subName.find("uvm_info") != std::string::npos);

        if (isInfoReport) {
            auto args = call.arguments();
            if (args.size() >= 3 && args[0] && args[2]) {
                // 1. Extract ID
                const auto* idArg = args[0];
                while (idArg->kind == slang::ast::ExpressionKind::Conversion) {
                    idArg = &idArg->as<slang::ast::ConversionExpression>().operand();
                }

                std::string idVal;
                if (idArg->kind == slang::ast::ExpressionKind::StringLiteral) {
                    idVal = std::string(idArg->as<slang::ast::StringLiteral>().getValue());
                }

                // 2. Extract Message (if literal)
                std::string msgVal;
                if (args[1]) {
                    const auto* msgArg = args[1];
                    while (msgArg->kind == slang::ast::ExpressionKind::Conversion) {
                        msgArg = &msgArg->as<slang::ast::ConversionExpression>().operand();
                    }
                    if (msgArg->kind == slang::ast::ExpressionKind::StringLiteral) {
                        msgVal = std::string(msgArg->as<slang::ast::StringLiteral>().getValue());
                    }
                }

                std::string upperId = toUpper(idVal);
                std::string upperMsg = toUpper(msgVal);

                bool isDebugContent = (upperId.find("DEBUG") != std::string::npos ||
                                       upperId.find("TRACE") != std::string::npos ||
                                       upperId.find("DUMP") != std::string::npos ||
                                       upperMsg.find("DEBUG") != std::string::npos ||
                                       upperMsg.find("TRACE") != std::string::npos);

                if (isDebugContent) {
                    // 3. Inspect Verbosity
                    const auto* verbArg = args[2];
                    while (verbArg->kind == slang::ast::ExpressionKind::Conversion) {
                        verbArg = &verbArg->as<slang::ast::ConversionExpression>().operand();
                    }

                    bool isLowVerbosity = false;
                    std::string verbName = verbArg->syntax ? std::string(verbArg->syntax->toString()) : "";

                    if (verbArg->kind == slang::ast::ExpressionKind::NamedValue) {
                        std::string symName = std::string(verbArg->as<slang::ast::NamedValueExpression>().symbol.name);
                        if (symName == "UVM_LOW" || symName == "UVM_NONE") {
                            isLowVerbosity = true;
                            verbName = symName;
                        }
                    } else if (verbArg->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                        auto val = verbArg->as<slang::ast::IntegerLiteral>().getValue();
                        if (val.isNegative() == false && val <= 100) {
                            isLowVerbosity = true;
                        }
                    } else if (verbName.find("UVM_LOW") != std::string::npos ||
                               verbName.find("UVM_NONE") != std::string::npos) {
                        isLowVerbosity = true;
                    }

                    if (isLowVerbosity) {
                        diags.addViolation(
                            "UVM-REP-005",
                            "LowVerbosityDebugReport",
                            "UVM report with debug/trace content (ID: '" + idVal +
                            "') is configured with verbosity '" + verbName + "'. In UVM logging best practices "
                            "(IEEE 1800.2 § 6.3), debug and trace messages must be configured with "
                            "UVM_HIGH or UVM_DEBUG to prevent flooding simulation logs during regression runs.",
                            core::Severity::Warning,
                            call.sourceRange.start()
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

class LowVerbosityDebugReport : public Rule {
public:
    std::string_view getId() const override { return "UVM-REP-005"; }
    std::string_view getName() const override { return "LowVerbosityDebugReport"; }
    std::string_view getDescription() const override {
        return "Flags UVM debug or trace messages configured with low verbosity (UVM_LOW or UVM_NONE).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        LowVerbosityDebugVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLowVerbosityDebugReport() {
    return std::make_unique<uvm::LowVerbosityDebugReport>();
}
} // namespace uvmcheck::rules
