#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/parsing/Token.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"

#include <filesystem>

namespace uvmcheck::rules::readability {

using namespace slang;
using namespace slang::syntax;
using parsing::Token;
using parsing::Trivia;
using parsing::TriviaKind;

namespace {

std::string_view trim(std::string_view s) {
    while (!s.empty() && (s.front() == ' ' || s.front() == '\t')) s.remove_prefix(1);
    while (!s.empty() && (s.back() == ' ' || s.back() == '\t' || s.back() == '\r' || s.back() == '\n'))
        s.remove_suffix(1);
    return s;
}

bool looksLikeCodeLine(std::string_view line) {
    if (line.empty()) return false;
    if (line.back() == ';') return true;
    return line == "begin" || line == "end" || line.starts_with("end ") || line == "endtask" ||
           line == "endfunction" || line == "join" || line == "join_none" || line == "join_any" ||
           line.starts_with("fork") || line.ends_with(" begin");
}

bool parsesCleanly(const std::string& text) {
    const std::string wrappers[][2] = {
        {"module m__; initial begin\n", "\nend\nendmodule\n"},
        {"class c__;\n", "\nendclass\n"},
    };
    for (const auto& w : wrappers) {
        SourceManager sm;
        auto tree = SyntaxTree::fromText(w[0] + text + w[1], sm);
        if (tree && tree->diagnostics().empty()) return true;
    }
    return false;
}

class CommentScanner : public SyntaxVisitor<CommentScanner> {
public:
    CommentScanner(const SourceManager& sm, int64_t minLines, core::DiagnosticEngine& diags) :
        sm(sm), minLines(minLines), diags(diags) {}

    void visitToken(Token token) {
        auto trivia = token.trivia();
        if (trivia.empty() || !token.location().valid() || sm.isMacroLoc(token.location())) return;
        std::string file(sm.getFileName(token.location()));
        if (std::filesystem::path(file).filename().string().starts_with("uvm_")) return;

        std::vector<std::string_view> group;
        const char* groupStart = nullptr;
        int eols = 0;
        auto flush = [&]() {
            check(token, group, groupStart);
            group.clear();
            groupStart = nullptr;
        };
        for (const auto& t : trivia) {
            if (t.kind == TriviaKind::LineComment) {
                std::string_view raw = t.getRawText();
                if (group.empty()) groupStart = raw.data();
                std::string_view body = raw.substr(2);
                group.push_back(trim(body));
                eols = 0;
            } else if (t.kind == TriviaKind::EndOfLine) {
                if (++eols > 1 && !group.empty()) flush(); // a blank line ends the block
            } else if (t.kind != TriviaKind::Whitespace) {
                if (!group.empty()) flush();
            }
        }
        if (!group.empty()) flush();
    }

private:
    const SourceManager& sm;
    int64_t minLines;
    core::DiagnosticEngine& diags;

    void check(Token token, const std::vector<std::string_view>& lines, const char* start) {
        int codeLines = 0;
        for (auto l : lines) codeLines += looksLikeCodeLine(l) ? 1 : 0;
        if (codeLines < minLines) return;
        std::string text;
        for (auto l : lines) {
            text += l;
            text += '\n';
        }
        if (!parsesCleanly(text)) return;
        // Trivia text points into the same buffer as the token; derive the comment's location.
        SourceLocation tokLoc = token.location();
        auto delta = static_cast<uint64_t>(token.rawText().data() - start);
        SourceLocation loc = tokLoc.offset() >= delta ? SourceLocation(tokLoc.buffer(), tokLoc.offset() - delta)
                                                      : tokLoc;
        diags.addViolation(
            "SV-RDB-003", "CommentedOutCode",
            std::to_string(lines.size()) + " lines of commented-out code. Version control keeps old code; "
                "delete it, or turn it into a switch if it is still needed.",
            core::Severity::Info, loc);
    }
};

} // namespace

class CommentedOutCode : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-003"; }
    std::string_view getName() const override { return "CommentedOutCode"; }
    std::string_view getDescription() const override {
        return "Flags blocks of // comments that parse as SystemVerilog statements or declarations "
               "(Sonar S125). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;
        CommentScanner scanner(*sm, swe::ruleOption("SV-RDB-003", "min_lines", 2), diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) tree->root().visit(scanner);
        }
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCommentedOutCode() {
    return std::make_unique<readability::CommentedOutCode>();
}
}
