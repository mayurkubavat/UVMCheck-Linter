#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::deadcode {

class EmptyForkJoinVisitor : public slang::syntax::SyntaxVisitor<EmptyForkJoinVisitor> {
public:
    explicit EmptyForkJoinVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
            if (block.items.empty()) {
                std::string joinKw = std::string(block.end.valueText());
                std::vector<core::FixItHint> fixIts;
                fixIts.push_back({
                    slang::SourceRange(block.begin.location(),
                                       block.end.location() + block.end.rawText().length()),
                    "",
                    "Remove empty fork..." + joinKw + " block"
                });

                diags.addViolation(
                    "SV-DED-010",
                    "EmptyForkJoin",
                    "Parallel 'fork ... " + joinKw +
                    "' block contains zero statements. Spawning empty concurrent fork blocks adds unnecessary delta cycles to the simulator event queue with zero functional behavior. Remove the empty block.",
                    core::Severity::Warning,
                    block.begin.location(),
                    fixIts
                );
            }
        }

        visitDefault(block);
    }

private:
    core::DiagnosticEngine& diags;
};

class EmptyForkJoin : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-010"; }
    std::string_view getName() const override { return "EmptyForkJoin"; }
    std::string_view getDescription() const override {
        return "Disallows empty fork...join, fork...join_any, or fork...join_none blocks that contain zero statements.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EmptyForkJoinVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyForkJoin() {
    return std::make_unique<deadcode::EmptyForkJoin>();
}
} // namespace uvmcheck::rules
