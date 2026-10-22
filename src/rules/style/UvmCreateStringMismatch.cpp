#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::style {

class FactoryCreateVisitor : public engine::SyntaxVisitor<FactoryCreateVisitor> {
public:
    explicit FactoryCreateVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.operatorToken.kind == slang::parsing::TokenKind::Equals) {
            std::string varName;
            if (expr.left->kind == slang::syntax::SyntaxKind::IdentifierName) {
                varName = std::string(expr.left->as<slang::syntax::IdentifierNameSyntax>().identifier.valueText());
            } else if (expr.left->kind == slang::syntax::SyntaxKind::MemberAccessExpression) {
                varName = std::string(expr.left->as<slang::syntax::MemberAccessExpressionSyntax>().name.valueText());
            }

            if (!varName.empty() && expr.right->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                const auto& inv = expr.right->as<slang::syntax::InvocationExpressionSyntax>();
                std::string callee = inv.left->toString();
                callee.erase(std::remove_if(callee.begin(), callee.end(), [](unsigned char c) { return std::isspace(c); }), callee.end());

                if (callee.ends_with("::create") || callee.ends_with(".create") || callee == "create") {
                    if (inv.arguments && !inv.arguments->parameters.empty()) {
                        const auto* arg0 = inv.arguments->parameters[0];
                        const slang::syntax::PropertyExprSyntax* propExpr = nullptr;
                        if (arg0->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                            propExpr = arg0->as<slang::syntax::OrderedArgumentSyntax>().expr;
                        } else if (arg0->kind == slang::syntax::SyntaxKind::NamedArgument) {
                            propExpr = arg0->as<slang::syntax::NamedArgumentSyntax>().expr;
                        }

                        if (propExpr) {
                            std::string rawVal = propExpr->toString();
                            // Trim whitespace
                            size_t start = rawVal.find_first_not_of(" \t\r\n");
                            size_t end = rawVal.find_last_not_of(" \t\r\n");
                            if (start != std::string::npos && end != std::string::npos) {
                                rawVal = rawVal.substr(start, end - start + 1);
                            }

                            if (rawVal.size() >= 2 && rawVal.front() == '"' && rawVal.back() == '"') {
                                std::string strVal = rawVal.substr(1, rawVal.size() - 2);

                                bool matches = (strVal == varName) ||
                                               (varName.starts_with("m_") && varName.substr(2) == strVal) ||
                                               (strVal.starts_with("m_") && strVal.substr(2) == varName);

                                if (!matches) {
                                    diags.addViolation(
                                        "UVM-STY-001",
                                        "UvmCreateStringMismatch",
                                        "UVM factory create instance string '\"" + strVal +
                                        "\"' does not match target variable identifier '" + varName +
                                        "'. In UVM guidelines, instance names must match the variable name to ensure predictable uvm_config_db scoping.",
                                        core::Severity::Warning,
                                        propExpr->getFirstToken().location()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class UvmCreateStringMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-STY-001"; }
    std::string_view getName() const override { return "UvmCreateStringMismatch"; }
    std::string_view getDescription() const override {
        return "String argument passed to type_id::create() must match the receiving variable name. "
               "Mismatched instance names break uvm_config_db hierarchy paths and targeted overrides.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        FactoryCreateVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmCreateStringMismatch() {
    return std::make_unique<style::UvmCreateStringMismatch>();
}
}
