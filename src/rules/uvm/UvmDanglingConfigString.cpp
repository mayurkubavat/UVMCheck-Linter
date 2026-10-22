#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class DanglingConfigStringVisitor : public engine::AstVisitor<DanglingConfigStringVisitor> {
public:
    explicit DanglingConfigStringVisitor(const uvmcheck::uvm::UvmModel& uvmModel, core::DiagnosticEngine& diags)
        : uvmModel(uvmModel), diags(diags) {}
    void handle(const slang::ast::CallExpression& call) {
        bool isConfigSet = false;
        if (call.subroutine.index() == 0) {
            const auto* sub = std::get<const slang::ast::SubroutineSymbol*>(call.subroutine);
            if (sub && sub->name == "set") {
                const auto* parentScope = sub->getParentScope();
                if (parentScope && parentScope->asSymbol().name.find("uvm_config_db") != std::string_view::npos) {
                    isConfigSet = true;
                }
            }
        }
        std::string subName = std::string(call.getSubroutineName());
        if (subName.find("uvm_config_db") != std::string::npos && subName.find("set") != std::string::npos) {
            isConfigSet = true;
        }
        if (call.syntax) {
            std::string syntaxStr = std::string(call.syntax->toString());
            if (syntaxStr.find("uvm_config_db") != std::string::npos && syntaxStr.find("set") != std::string::npos) {
                isConfigSet = true;
            }
        }

        if (isConfigSet) {
            auto args = call.arguments();
            // uvm_config_db#(T)::set(cntxt, "inst_name", "field_name", value)
            if (args.size() >= 4) {
                const auto* instArg = args[1];
                while (instArg->kind == slang::ast::ExpressionKind::Conversion) {
                    instArg = &instArg->as<slang::ast::ConversionExpression>().operand();
                }

                if (instArg->kind == slang::ast::ExpressionKind::StringLiteral) {
                    std::string instPattern = std::string(instArg->as<slang::ast::StringLiteral>().getValue());

                    if (!instPattern.empty() && instPattern != "" && instPattern != "*" &&
                        !uvmModel.getComponentPaths().empty()) {

                        if (!uvmModel.matchesAnyComponentPath(instPattern)) {
                            diags.addViolation(
                                "UVM-DAG-001",
                                "UvmDanglingConfigString",
                                "Dangling UVM config_db path: string pattern '" + instPattern +
                                "' matches 0 instantiated components in the static verification hierarchy DAG. "
                                "Typographical errors in config_db target strings cause configurations to be silently dropped.",
                                core::Severity::Error,
                                instArg->sourceRange.start()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(call);
    }

private:
    const uvmcheck::uvm::UvmModel& uvmModel;
    core::DiagnosticEngine& diags;
};

class UvmDanglingConfigString : public Rule {
public:
    std::string_view getId() const override { return "UVM-DAG-001"; }
    std::string_view getName() const override { return "UvmDanglingConfigString"; }
    std::string_view getDescription() const override {
        return "Static UVM component DAG path matching: detects glob strings in uvm_config_db::set() that match "
               "zero instantiated components in the verification hierarchy. Typographical errors in string paths cause configurations to be silently dropped.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }
    uint32_t getProfiles() const override {
        return static_cast<uint32_t>(RuleProfile::Soundness) | static_cast<uint32_t>(RuleProfile::Methodology);
    }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        const auto& uvmModel = ctx.uvmModel();
        auto& diags = ctx.diags();
        DanglingConfigStringVisitor visitor(uvmModel, diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmDanglingConfigString() {
    return std::make_unique<uvm::UvmDanglingConfigString>();
}
}
