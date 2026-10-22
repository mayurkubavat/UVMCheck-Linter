#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::style {

class BannerFormattingVisitor : public engine::SyntaxVisitor<BannerFormattingVisitor> {
public:
    explicit BannerFormattingVisitor(core::DiagnosticEngine& diags, bool checkMethods = true, bool checkSections = true)
        : diags(diags), checkMethods(checkMethods), checkSections(checkSections) {}

    void checkTrivia(const slang::parsing::Trivia& trivia, slang::SourceLocation loc) {
        if (trivia.kind == slang::parsing::TriviaKind::LineComment) {
            std::string_view text = trivia.getRawText();

            // Trim trailing \r or \n
            while (!text.empty() && (text.back() == '\r' || text.back() == '\n' || text.back() == ' ')) {
                text.remove_suffix(1);
            }

            // Check double-equal section banners: //========
            if (checkSections && text.starts_with("//==")) {
                bool allEquals = true;
                for (size_t i = 2; i < text.size(); ++i) {
                    if (text[i] != '=') {
                        allEquals = false;
                        break;
                    }
                }
                if (allEquals && (text.size() < 70 || text.size() > 85)) {
                    diags.addViolation(
                        "SV-STY-008",
                        "SectionHeaderBanner",
                        "Non-standard section header banner width (" + std::to_string(text.size()) +
                        " characters). Major section divider banners must standardize to 70–80 columns (e.g. 78 characters: //===...).",
                        core::Severity::Info,
                        loc
                    );
                }
            }
            // Check dashed separators: //--------
            else if (checkMethods && text.starts_with("//--")) {
                bool allDashes = true;
                for (size_t i = 2; i < text.size(); ++i) {
                    if (text[i] != '-') {
                        allDashes = false;
                        break;
                    }
                }
                if (allDashes && (text.size() < 70 || text.size() > 85)) {
                    diags.addViolation(
                        "SV-STY-007",
                        "MethodBannerStyle",
                        "Ragged or non-standard method/separator banner (" + std::to_string(text.size()) +
                        " characters). In standard style guidelines, method separators must span 70–80 columns (e.g. 78 hyphens: //---...).",
                        core::Severity::Info,
                        loc
                    );
                }
            }
        }
    }

    void visitToken(slang::parsing::Token token) {
        for (const auto& trivia : token.trivia()) {
            checkTrivia(trivia, token.location());
        }
    }

private:
    core::DiagnosticEngine& diags;
    bool checkMethods;
    bool checkSections;
};

class MethodBannerStyle : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-007"; }
    std::string_view getName() const override { return "MethodBannerStyle"; }
    std::string_view getDescription() const override {
        return "Standardizes method separator pre-banners. Enforces 70–80 character uniform dashed separator lines "
               "and flags ragged or arbitrary-width comment separators.";
    }
    std::string_view getCategory() const override { return "Documentation"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        BannerFormattingVisitor visitor(diags, true, false);
        co_await ctx.visitSyntax(visitor);
    }
};

class SectionHeaderBanner : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-008"; }
    std::string_view getName() const override { return "SectionHeaderBanner"; }
    std::string_view getDescription() const override {
        return "Standardizes major section divider banners to uniform 70–80 character widths. "
               "Prevents ragged ASCII banners across verification packages and testbench files.";
    }
    std::string_view getCategory() const override { return "Documentation"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        BannerFormattingVisitor visitor(diags, false, true);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMethodBannerStyle() {
    return std::make_unique<style::MethodBannerStyle>();
}
std::unique_ptr<Rule> createSectionHeaderBanner() {
    return std::make_unique<style::SectionHeaderBanner>();
}
}
