#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include <unordered_set>

namespace uvmcheck::rules::style {

class OneStatementPerLineVisitor : public slang::syntax::SyntaxVisitor<OneStatementPerLineVisitor> {
public:
    OneStatementPerLineVisitor(const slang::SourceManager& sm, core::DiagnosticEngine& diags)
        : sm(sm), diags(diags) {}

    void checkItems(const slang::syntax::SyntaxList<slang::syntax::SyntaxNode>& items) {
        if (items.size() < 2) return;

        size_t lastReportedLine = 0;
        for (size_t i = 0; i + 1 < items.size(); ++i) {
            const auto* curr = items[i];
            const auto* next = items[i + 1];

            if (curr && next) {
                auto currLoc = curr->getLastToken().location();
                auto nextLoc = next->getFirstToken().location();

                if (currLoc.buffer() == nextLoc.buffer()) {
                    size_t currLine = sm.getLineNumber(currLoc);
                    size_t nextLine = sm.getLineNumber(nextLoc);

                    if (currLine == nextLine && currLine != lastReportedLine) {
                        lastReportedLine = currLine;
                        diags.addViolation(
                            "SV-STY-011",
                            "OneStatementPerLine",
                            "Multiple procedural statements found on line " + std::to_string(currLine) +
                            ". By convention, each statement must reside on its own line to ensure readable diffs and deterministic interactive debugging.",
                            core::Severity::Warning,
                            next->getFirstToken().location()
                        );
                    }
                }
            }
        }
    }

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        checkItems(block.items);
        visitDefault(block);
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& fn) {
        checkItems(fn.items);
        visitDefault(fn);
    }

private:
    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
};

class OneStatementPerLine : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-011"; }
    std::string_view getName() const override { return "OneStatementPerLine"; }
    std::string_view getDescription() const override {
        return "Prohibit multiple procedural statements on the same line separated by semicolons. "
               "Enforces clean formatting and deterministic single-stepping in simulation debuggers.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        OneStatementPerLineVisitor visitor(*compilation.getSourceManager(), diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOneStatementPerLine() {
    return std::make_unique<style::OneStatementPerLine>();
}
}
