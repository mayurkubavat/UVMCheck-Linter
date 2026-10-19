#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/parsing/Token.h"
#include "slang/parsing/TokenKind.h"
#include <algorithm>
#include <array>
#include <string>

namespace uvmcheck::rules::style {

class CommentTriviaVisitor : public slang::syntax::SyntaxVisitor<CommentTriviaVisitor> {
public:
    explicit CommentTriviaVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void visitToken(slang::parsing::Token token) {
        for (const auto& trivia : token.trivia()) {
            if (trivia.kind == slang::parsing::TriviaKind::LineComment ||
                trivia.kind == slang::parsing::TriviaKind::BlockComment) {
                std::string_view text = trivia.getRawText();
                static constexpr std::array<std::string_view, 5> tags = {
                    "TODO", "FIXME", "ICEBOX", "HACK", "XXX"
                };

                for (const auto tag : tags) {
                    auto pos = text.find(tag);
                    if (pos != std::string_view::npos) {
                        std::string snippet(text);
                        while (!snippet.empty() && (snippet.back() == '\r' || snippet.back() == '\n')) {
                            snippet.pop_back();
                        }
                        if (snippet.size() > 80) {
                            snippet = snippet.substr(0, 77) + "...";
                        }

                        slang::SourceLocation loc = trivia.getExplicitLocation().value_or(token.location());
                        diags.addViolation(
                            "SV-STY-003",
                            "TodoCommentTracker",
                            "Unresolved development tag '" + std::string(tag) + "' found in comment: " +
                            snippet + ". Track and resolve all temporary FIXME, TODO, and ICEBOX items prior to verification closure.",
                            core::Severity::Info,
                            loc
                        );
                        break;
                    }
                }
            }
        }
    }

private:
    core::DiagnosticEngine& diags;
};

class TodoCommentTracker : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-003"; }
    std::string_view getName() const override { return "TodoCommentTracker"; }
    std::string_view getDescription() const override {
        return "Tracks unresolved TODO, FIXME, ICEBOX, HACK, and XXX comment markers across the codebase. "
               "Ensures temporary developer placeholders are addressed before tape-out or milestone delivery.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        CommentTriviaVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTodoCommentTracker() {
    return std::make_unique<style::TodoCommentTracker>();
}
}
