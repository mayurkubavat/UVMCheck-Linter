#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include <unordered_set>

namespace uvmcheck::rules::smells {

class ScratchpadVisitor : public engine::AstVisitor<ScratchpadVisitor> {
public:
    explicit ScratchpadVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ForLoopStatement& forLoop) {
        for (const auto* initExpr : forLoop.initializers) {
            if (initExpr->kind == slang::ast::ExpressionKind::Assignment) {
                const auto& assign = initExpr->as<slang::ast::AssignmentExpression>();
                const auto* sym = assign.left().getSymbolReference();
                if (sym && sym->kind == slang::ast::SymbolKind::ClassProperty) {
                    diags.addViolation(
                        "SV-SML-001",
                        "ScratchpadClassProperty",
                        "Loop induction variable '" + std::string(sym->name) +
                        "' is declared as a class property rather than scoped locally to the loop or routine ('for (int " +
                        std::string(sym->name) + " = 0; ...)'). "
                        "Class-scoped loop counters introduce severe multi-threaded race conditions when routines execute concurrently in fork..join.",
                        core::Severity::Warning,
                        assign.sourceRange.start()
                    );
                    flaggedProperties.insert(sym);
                }
            }
        }
        visitDefault(forLoop);
    }

    void handle(const slang::ast::ClassPropertySymbol& prop) {
        static const std::unordered_set<std::string_view> scratchpadNames = {
            "i", "j", "k", "idx", "index", "temp", "tmp", "scratch", "scratchpad", "loop_cnt"
        };

        if (scratchpadNames.count(prop.name) > 0 && flaggedProperties.count(&prop) == 0) {
            bool isStatic = (prop.lifetime == slang::ast::VariableLifetime::Static);
            bool isConst = prop.flags.has(slang::ast::VariableFlags::Const);
            if (!isStatic && !isConst) {
                diags.addViolation(
                    "SV-SML-001",
                    "ScratchpadClassProperty",
                    "Scratchpad variable '" + std::string(prop.name) +
                    "' is declared as a class property. Temporary variables and loop indices must be declared locally within the routine stack "
                    "to prevent heap bloat and re-entrancy race conditions.",
                    core::Severity::Warning,
                    prop.location
                );
            }
        }
        visitDefault(prop);
    }

private:
    std::unordered_set<const slang::ast::Symbol*> flaggedProperties;
    core::DiagnosticEngine& diags;
};

class ScratchpadClassProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-SML-001"; }
    std::string_view getName() const override { return "ScratchpadClassProperty"; }
    std::string_view getDescription() const override {
        return "Disallow declaring scratchpad variables or loop induction counters as class properties. Move declarations to method stack frames to ensure thread safety and re-entrancy.";
    }
    std::string_view getCategory() const override { return "Smell"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ScratchpadVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::smells

namespace uvmcheck::rules {
std::unique_ptr<Rule> createScratchpadClassProperty() {
    return std::make_unique<smells::ScratchpadClassProperty>();
}
}
