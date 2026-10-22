#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::random {

class RandcSyntaxVisitor : public engine::SyntaxVisitor<RandcSyntaxVisitor> {
public:
    explicit RandcSyntaxVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DataDeclarationSyntax& decl) {
        bool hasRandc = false;
        slang::SourceLocation randcLoc;
        for (auto mod : decl.modifiers) {
            if (mod.kind == slang::parsing::TokenKind::RandCKeyword) {
                hasRandc = true;
                randcLoc = mod.location();
                break;
            }
        }

        if (hasRandc) {
            for (const auto* declItem : decl.declarators) {
                if (!declItem) continue;

                bool isDynamic = false;
                for (const auto* dim : declItem->dimensions) {
                    if (dim && dim->kind == slang::syntax::SyntaxKind::VariableDimension) {
                        const auto& vdim = dim->as<slang::syntax::VariableDimensionSyntax>();
                        if (!vdim.specifier ||
                            vdim.specifier->kind == slang::syntax::SyntaxKind::QueueDimensionSpecifier) {
                            isDynamic = true;
                            break;
                        }
                    }
                }

                if (isDynamic) {
                    std::string varName = std::string(declItem->name.valueText());
                    diags.addViolation(
                        "SV-RND-020",
                        "RandcInDynamicContainer",
                        "Variable '" + varName + "' is declared as 'randc' on a dynamic container (dynamic array or queue). "
                        "In SystemVerilog (IEEE 1800 § 18.4.1), 'randc' cyclic random variables "
                        "are strictly prohibited on dynamic arrays, queues, and associative arrays because dynamic container resizing invalidates cyclic permutation tables.",
                        core::Severity::Error,
                        randcLoc.valid() ? randcLoc : declItem->name.location()
                    );
                }
            }
        }
        visitDefault(decl);
    }

private:
    core::DiagnosticEngine& diags;
};

class RandcInDynamicContainer : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-020"; }
    std::string_view getName() const override { return "RandcInDynamicContainer"; }
    std::string_view getDescription() const override {
        return "Disallows declaring 'randc' cyclic variables on dynamic arrays, queues, or associative arrays (IEEE 1800 § 18.4.1).";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        const auto& uvmModel = ctx.uvmModel();
        auto& diags = ctx.diags();
        // 1. Check AST symbols
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    if (prop.getRandMode() == slang::ast::RandMode::RandC) {
                        const auto& type = prop.getType();
                        if (type.isQueue() || type.isDynamicallySizedArray() || type.isAssociativeArray()) {
                            diags.addViolation(
                                "SV-RND-020",
                                "RandcInDynamicContainer",
                                "Class property '" + std::string(prop.name) + "' is declared as 'randc' on a dynamic container. "
                                "In SystemVerilog (IEEE 1800 § 18.4.1), 'randc' cyclic variables "
                                "are strictly prohibited on dynamic arrays, queues, and associative arrays.",
                                core::Severity::Error,
                                prop.location
                            );
                        }
                    }
                }
            }
        }

        // 2. Syntax-based check for robust coverage
        RandcSyntaxVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandcInDynamicContainer() {
    return std::make_unique<random::RandcInDynamicContainer>();
}
} // namespace uvmcheck::rules
