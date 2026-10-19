#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::style {

class MaxLineLength : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-016"; }
    std::string_view getName() const override { return "MaxLineLength"; }
    std::string_view getDescription() const override {
        return "Flags source lines exceeding 120 characters to preserve readability across split-view editors and terminal reviews.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        constexpr size_t kMaxColumns = 120;

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

                        size_t lineLen = lineEnd - lineStart;
                        if (lineLen > kMaxColumns) {
                            slang::SourceLocation loc(bufferId, static_cast<uint64_t>(lineStart + kMaxColumns));
                            diags.addViolation(
                                "SV-STY-016",
                                "MaxLineLength",
                                "Line " + std::to_string(lineNum) + " length (" + std::to_string(lineLen) +
                                " characters) exceeds maximum recommended threshold of 120 characters. Wrap long expressions or break statements across lines.",
                                core::Severity::Info,
                                loc
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
std::unique_ptr<Rule> createMaxLineLength() {
    return std::make_unique<style::MaxLineLength>();
}
}
