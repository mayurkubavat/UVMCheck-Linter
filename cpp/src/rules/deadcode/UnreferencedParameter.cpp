#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/Scope.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include <unordered_map>
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::deadcode {

class ParamDeclVisitor : public engine::AstVisitor<ParamDeclVisitor> {
public:
    struct ParamInfo {
        slang::SourceLocation loc;
        std::string scopeName;
    };

    void handle(const slang::ast::ParameterSymbol& param) {
        if (!param.name.empty() && !param.name.starts_with("$")) {
            std::string pName = std::string(param.name);
            std::string sName = param.getParentScope() ? std::string(param.getParentScope()->asSymbol().name) : "global";
            if (sName.empty()) sName = "global";
            declaredParams[pName] = { param.location, sName };
        }
        visitDefault(param);
    }

    void handle(const slang::ast::NamedValueExpression& expr) {
        referencedParams.insert(std::string(expr.symbol.name));
        visitDefault(expr);
    }

    std::unordered_map<std::string, ParamInfo> declaredParams;
    std::unordered_set<std::string> referencedParams;
};

// Names elaboration evaluates rather than binds as values: dimensions
// (`logic [W-1:0] q`) and the headers of generate constructs. A width
// parameter used only there is referenced. The native engine counts the same.
class EvaluatedNamesVisitor : public engine::SyntaxVisitor<EvaluatedNamesVisitor> {
public:
    void handle(const slang::syntax::VariableDimensionSyntax& dim) {
        collect(dim);
        visitDefault(dim);
    }
    void handle(const slang::syntax::LoopGenerateSyntax& loop) {
        collect(*loop.initialExpr);
        collect(*loop.stopExpr);
        collect(*loop.iterationExpr);
        visitDefault(loop);
    }
    void handle(const slang::syntax::IfGenerateSyntax& gen) {
        collect(*gen.condition);
        visitDefault(gen);
    }
    void handle(const slang::syntax::CaseGenerateSyntax& gen) {
        collect(*gen.condition);
        for (const auto* item : gen.items) {
            if (item->kind == slang::syntax::SyntaxKind::StandardCaseItem)
                for (const auto* e : item->as<slang::syntax::StandardCaseItemSyntax>().expressions) collect(*e);
        }
        visitDefault(gen);
    }

    std::unordered_set<std::string> names;

private:
    void collect(const slang::syntax::SyntaxNode& n) {
        for (size_t i = 0, k = n.getChildCount(); i < k; i++) {
            if (const auto* kid = n.childNode(i)) {
                collect(*kid);
            } else {
                const auto tok = n.childToken(i);
                if (tok.kind == slang::parsing::TokenKind::Identifier) names.insert(std::string(tok.valueText()));
            }
        }
    }
};

class UnreferencedParameter : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-012"; }
    std::string_view getName() const override { return "UnreferencedParameter"; }
    std::string_view getDescription() const override {
        return "Flags parameter and localparam declarations that are never referenced within their scope or testbench. "
               "Unused parameters create clutter and indicate dead or abandoned configurations.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ParamDeclVisitor visitor;
        co_await ctx.visitAst(visitor);
        EvaluatedNamesVisitor evaluated;
        co_await ctx.visitSyntax(evaluated);
        visitor.referencedParams.insert(evaluated.names.begin(), evaluated.names.end());

        for (const auto& [name, info] : visitor.declaredParams) {
            if (visitor.referencedParams.find(name) == visitor.referencedParams.end()) {
                // Avoid flagging standard UVM or simulator built-ins
                if (name == "UVM_NONE" || name == "UVM_LOW" || name == "UVM_MEDIUM" ||
                    name == "UVM_HIGH" || name == "UVM_FULL" || name == "UVM_DEBUG") {
                    continue;
                }

                diags.addViolation(
                    "SV-DED-012",
                    "UnreferencedParameter",
                    "Parameter '" + name + "' is declared in scope '" + info.scopeName + "' but never referenced. "
                    "Unreferenced parameters complicate module and VIP interfaces, and often indicate forgotten "
                    "configuration logic or obsolete constants.",
                    core::Severity::Warning,
                    info.loc
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreferencedParameter() {
    return std::make_unique<deadcode::UnreferencedParameter>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnreferencedParameter)
