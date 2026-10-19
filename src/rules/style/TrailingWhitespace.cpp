#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::style {

class TrailingWhitespace : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-015"; }
    std::string_view getName() const override { return "TrailingWhitespace"; }
    std::string_view getDescription() const override {
        return "Disallows trailing whitespace (spaces and tabs) at the end of lines in source files.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (!tree) continue;
            const auto& sm = tree->sourceManager();
            for (auto bufferId : tree->getSourceBufferIds()) {
                std::string_view text = sm.getSourceText(bufferId);
                if (text.empty()) continue;

                size_t lineStart = 0;
                size_t lineNum = 1;
                size_t n = text.size();

            for (size_t i = 0; i <= n; ++i) {
                if (i == n || text[i] == '\n') {
                    size_t lineEnd = i;
                    if (lineEnd > lineStart && text[lineEnd - 1] == '\r') {
                        lineEnd--;
                    }

                    size_t wsStart = lineEnd;
                    while (wsStart > lineStart && (text[wsStart - 1] == ' ' || text[wsStart - 1] == '\t')) {
                        wsStart--;
                    }

                    if (wsStart < lineEnd) {
                        size_t wsCount = lineEnd - wsStart;
                        slang::SourceLocation startLoc(bufferId, static_cast<uint64_t>(wsStart));
                        slang::SourceLocation endLoc(bufferId, static_cast<uint64_t>(lineEnd));
                        slang::SourceRange wsRange(startLoc, endLoc);

                        std::vector<core::FixItHint> fixIts;
                        if (startLoc.valid() && endLoc.valid()) {
                            fixIts.push_back({
                                wsRange,
                                "",
                                "Remove trailing whitespace"
                            });
                        }

                        diags.addViolation(
                            "SV-STY-015",
                            "TrailingWhitespace",
                            "Line " + std::to_string(lineNum) + " contains " + std::to_string(wsCount) +
                            " trailing whitespace character(s). Trailing whitespace creates spurious git diffs and violates formatting hygiene.",
                            core::Severity::Info,
                            startLoc,
                            std::move(fixIts)
                        );
                    }

                    lineStart = i + 1;
                    lineNum++;
                }
            }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTrailingWhitespace() {
    return std::make_unique<style::TrailingWhitespace>();
}
}
