#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include <unordered_set>

namespace uvmcheck::rules::style {

class OneStatementPerLineVisitor : public engine::SyntaxVisitor<OneStatementPerLineVisitor> {
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
                        const auto lastTok = curr->getLastToken();
                        diags.addViolation(
                            "SV-STY-011",
                            "OneStatementPerLine",
                            "Multiple procedural statements found on line " + std::to_string(currLine) +
                            ". By convention, each statement must reside on its own line to ensure readable diffs and deterministic interactive debugging.",
                            core::Severity::Warning,
                            next->getFirstToken().location(),
                            splitFix(lastTok.location() + lastTok.rawText().size(), nextLoc)
                        );
                    }
                }
            }
        }
    }

    // Moves the next statement onto its own line at the same indent. Only plain spaces
    // may sit between the two statements; a comment there is left for a person to place.
    std::vector<core::FixItHint> splitFix(slang::SourceLocation from, slang::SourceLocation to) const {
        if (!sm.isFileLoc(from) || !sm.isFileLoc(to) || from.buffer() != to.buffer()) return {};
        std::string_view text = sm.getSourceText(from.buffer());
        if (to.offset() > text.size() || from.offset() > to.offset()) return {};
        for (size_t i = from.offset(); i < to.offset(); ++i) {
            if (text[i] != ' ' && text[i] != '\t') return {};
        }
        size_t lineStart = from.offset();
        while (lineStart > 0 && text[lineStart - 1] != '\n') --lineStart;
        size_t indentEnd = lineStart;
        while (indentEnd < text.size() && (text[indentEnd] == ' ' || text[indentEnd] == '\t')) ++indentEnd;
        return {{slang::SourceRange(from, to), "\n" + std::string(text.substr(lineStart, indentEnd - lineStart)),
                 "Put the next statement on its own line"}};
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

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& compilation = ctx.compilation();
        auto& diags = ctx.diags();
        OneStatementPerLineVisitor visitor(*compilation.getSourceManager(), diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOneStatementPerLine() {
    return std::make_unique<style::OneStatementPerLine>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createOneStatementPerLine)
