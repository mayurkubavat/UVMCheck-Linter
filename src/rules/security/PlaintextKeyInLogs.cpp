#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::security {

class PlaintextKeyVisitor : public slang::ast::ASTVisitor<PlaintextKeyVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit PlaintextKeyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        std::string subName = std::string(call.getSubroutineName());
        if (subName.starts_with('$')) subName = subName.substr(1);
        if (subName == "sformatf" || subName == "display" || subName == "write" || subName == "writeh") {
            auto args = call.arguments();
            if (args.size() >= 2) {
                // Check if first arg is format string
                const auto* fmtArg = args[0];
                while (fmtArg->kind == slang::ast::ExpressionKind::Conversion) {
                    fmtArg = &fmtArg->as<slang::ast::ConversionExpression>().operand();
                }

                if (fmtArg->kind == slang::ast::ExpressionKind::StringLiteral) {
                    std::string fmtStr = std::string(fmtArg->as<slang::ast::StringLiteral>().getValue());

                    // Check if format string prints hex or binary (%h, %x, %b, %s)
                    if (fmtStr.find('%') != std::string::npos) {
                        for (size_t i = 1; i < args.size(); ++i) {
                            const auto* dataArg = args[i];
                            while (dataArg->kind == slang::ast::ExpressionKind::Conversion) {
                                dataArg = &dataArg->as<slang::ast::ConversionExpression>().operand();
                            }

                            std::string argName;
                            if (dataArg->syntax) {
                                argName = std::string(dataArg->syntax->toString());
                            }

                            std::string lowerArg = toLower(argName);
                            // Identify sensitive cryptographic assets
                            if (lowerArg.find("key") != std::string::npos ||
                                lowerArg.find("secret") != std::string::npos ||
                                lowerArg.find("seed") != std::string::npos ||
                                lowerArg.find("token") != std::string::npos ||
                                lowerArg.find("priv_") != std::string::npos) {

                                diags.addViolation(
                                    "SEC-LOG-001",
                                    "PlaintextKeyInLogs",
                                    "Potential plaintext logging of sensitive cryptographic asset '" + argName +
                                    "' via $" + subName + "(). Printing keys, seeds, or root-of-trust secrets in simulation "
                                    "transcripts violates hardware security standards (ISO/SAE 21434 / Common Criteria). "
                                    "Mask or omit sensitive secrets from regression logs.",
                                    core::Severity::Warning,
                                    dataArg->sourceRange.start()
                                );
                            }
                        }
                    }
                }
            }
        }

        visitDefault(call);
    }

private:
    static std::string toLower(std::string s) {
        for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        return s;
    }

    core::DiagnosticEngine& diags;
};

class PlaintextKeyInLogs : public Rule {
public:
    std::string_view getId() const override { return "SEC-LOG-001"; }
    std::string_view getName() const override { return "PlaintextKeyInLogs"; }
    std::string_view getDescription() const override {
        return "Detects plaintext logging of sensitive cryptographic assets (keys, seeds, secrets) via $sformatf/$display. "
               "Logging secrets compromises hardware security and violates ISO/SAE 21434 regression standards.";
    }
    std::string_view getCategory() const override { return "Security"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        PlaintextKeyVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::security

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPlaintextKeyInLogs() {
    return std::make_unique<security::PlaintextKeyInLogs>();
}
}
