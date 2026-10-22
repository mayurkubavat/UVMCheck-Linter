#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class RalAssignmentVisitor : public engine::AstVisitor<RalAssignmentVisitor> {
public:
    RalAssignmentVisitor(const uvmcheck::uvm::UvmModel& uvmModel, core::DiagnosticEngine& diags)
        : uvmModel(uvmModel), diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        if (assign.left().kind == slang::ast::ExpressionKind::MemberAccess) {
            const auto& memberAccess = assign.left().as<slang::ast::MemberAccessExpression>();
            std::string_view memberName = memberAccess.member.name;

            if (memberName == "m_value" || memberName == "m_mirrored" || memberName == "m_desired") {
                const auto& targetType = *memberAccess.value().type;
                bool isRalType = false;

                if (targetType.isClass()) {
                    std::string typeName(targetType.name);
                    if (typeName.find("reg") != std::string::npos ||
                        typeName.find("field") != std::string::npos ||
                        uvmModel.isDerivedFrom(targetType, "uvm_reg") ||
                        uvmModel.isDerivedFrom(targetType, "uvm_reg_field") ||
                        uvmModel.isDerivedFrom(targetType, "uvm_reg_block")) {
                        isRalType = true;
                    }
                }

                if (isRalType) {
                    std::string regName = memberAccess.value().syntax ? memberAccess.value().syntax->toString() : "reg";
                    diags.addViolation(
                        "UVM-RAL-001",
                        "DirectRalMirrorTampering",
                        "Direct assignment to internal RAL property '" + std::string(memberName) +
                        "' on register/field handle '" + regName + "'. "
                        "Bypassing the standard UVM RAL API avoids access policy enforcement (RO, WO, W1C), "
                        "prediction callbacks, coverage sampling, and mirror consistency checks. "
                        "Use 'predict()' to update the mirrored model state, or execute bus read/write operations.",
                        core::Severity::Error,
                        assign.sourceRange.start()
                    );
                }
            }
        }
        visitDefault(assign);
    }

private:
    const uvmcheck::uvm::UvmModel& uvmModel;
    core::DiagnosticEngine& diags;
};

class DirectRalMirrorTampering : public Rule {
public:
    std::string_view getId() const override { return "UVM-RAL-001"; }
    std::string_view getName() const override { return "DirectRalMirrorTampering"; }
    std::string_view getDescription() const override {
        return "Direct assignment to internal RAL fields (m_value, m_mirrored, m_desired) bypasses register callbacks, "
               "access policy checks, and prediction pipelines. Use predict() or bus operations instead.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        const auto& uvmModel = ctx.uvmModel();
        auto& diags = ctx.diags();
        RalAssignmentVisitor visitor(uvmModel, diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDirectRalMirrorTampering() {
    return std::make_unique<uvm::DirectRalMirrorTampering>();
}
}
