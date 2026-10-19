#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/types/Type.h"

namespace uvmcheck::rules::types {

class RedundantCastVisitor : public slang::ast::ASTVisitor<RedundantCastVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit RedundantCastVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.getSubroutineName() == "$cast") {
            auto args = call.arguments();
            if (args.size() >= 2 && args[0] && args[1]) {
                const auto* dest = args[0];
                const auto* src = args[1];

                while (dest->kind == slang::ast::ExpressionKind::Conversion) {
                    dest = &dest->as<slang::ast::ConversionExpression>().operand();
                }
                while (src->kind == slang::ast::ExpressionKind::Conversion) {
                    src = &src->as<slang::ast::ConversionExpression>().operand();
                }

                if (dest->type && src->type && dest->type->isMatching(*src->type)) {
                    diags.addViolation(
                        "SV-TYP-008",
                        "RedundantCastToSameType",
                        "Redundant dynamic cast '$cast(...)': destination and source both share identical static type '" +
                        dest->type->toString() + "'. In SystemVerilog typing rules (IEEE 1800 § 6.24), dynamic casting between "
                        "identical types is unnecessary; use standard assignment '=' instead.",
                        core::Severity::Warning,
                        call.sourceRange.start()
                    );
                }
            }
        }
        visitDefault(call);
    }

    void handle(const slang::ast::ConversionExpression& conv) {
        if (!conv.isImplicit() && conv.type) {
            const auto* op = &conv.operand();
            while (op->kind == slang::ast::ExpressionKind::Conversion && op->as<slang::ast::ConversionExpression>().isImplicit()) {
                op = &op->as<slang::ast::ConversionExpression>().operand();
            }

            if (op->type && conv.type->isMatching(*op->type)) {
                diags.addViolation(
                    "SV-TYP-008",
                    "RedundantCastToSameType",
                    "Redundant static cast to identical type '" + conv.type->toString() + "'. "
                    "In SystemVerilog typing rules (IEEE 1800 § 6.24), casting an expression to its already established "
                    "static type adds unnecessary verbosity.",
                    core::Severity::Info,
                    conv.sourceRange.start()
                );
            }
        }
        visitDefault(conv);
    }

private:
    core::DiagnosticEngine& diags;
};

class RedundantCastToSameType : public Rule {
public:
    std::string_view getId() const override { return "SV-TYP-008"; }
    std::string_view getName() const override { return "RedundantCastToSameType"; }
    std::string_view getDescription() const override {
        return "Flags dynamic '$cast' calls or static type casts where the source and destination already have identical static types.";
    }
    std::string_view getCategory() const override { return "Types"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        RedundantCastVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::types

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRedundantCastToSameType() {
    return std::make_unique<types::RedundantCastToSameType>();
}
} // namespace uvmcheck::rules
