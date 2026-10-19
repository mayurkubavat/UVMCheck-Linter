#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::style {

class SpaceKeywordParenVisitor : public slang::syntax::SyntaxVisitor<SpaceKeywordParenVisitor> {
public:
    explicit SpaceKeywordParenVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkSpace(slang::parsing::Token keyword, slang::parsing::Token openParen) {
        if (!keyword.isMissing() && !openParen.isMissing() &&
            keyword.location().buffer() == openParen.location().buffer()) {
            size_t kwEnd = keyword.location().offset() + keyword.rawText().size();
            size_t parenStart = openParen.location().offset();
            if (kwEnd == parenStart) {
                std::string kwText = std::string(keyword.valueText());
                diags.addViolation(
                    "SV-STY-012",
                    "SpaceKeywordParen",
                    "Missing space between '" + kwText +
                    "' keyword and opening parenthesis '('. "
                    "Common SystemVerilog formatting requires a space (e.g. '" + kwText + " (...)').",
                    core::Severity::Info,
                    keyword.location()
                );
            }
        }
    }

    void handle(const slang::syntax::ConditionalStatementSyntax& stmt) {
        checkSpace(stmt.ifKeyword, stmt.openParen);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::LoopStatementSyntax& stmt) {
        checkSpace(stmt.repeatOrWhile, stmt.openParen);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& stmt) {
        checkSpace(stmt.whileKeyword, stmt.openParen);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForLoopStatementSyntax& stmt) {
        checkSpace(stmt.forKeyword, stmt.openParen);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& stmt) {
        if (stmt.loopList) {
            checkSpace(stmt.keyword, stmt.loopList->openParen);
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::CaseStatementSyntax& stmt) {
        checkSpace(stmt.caseKeyword, stmt.openParen);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class SpaceKeywordParen : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-012"; }
    std::string_view getName() const override { return "SpaceKeywordParen"; }
    std::string_view getDescription() const override {
        return "Requires a single space between control keywords (if, for, foreach, while, case) and opening parenthesis. "
               "Enforces common SystemVerilog code formatting guidelines.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        SpaceKeywordParenVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSpaceKeywordParen() {
    return std::make_unique<style::SpaceKeywordParen>();
}
}
