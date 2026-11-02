#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/text/SourceManager.h"
#include <cctype>
#include <filesystem>

namespace uvmcheck::rules::style {

// Wraps the whole file in `ifndef/`define/`endif as one edit, so the opening and closing
// halves can never be applied apart. Unsafe: a header meant to be included more than once
// (for instance to stamp out code per define) stops working.
std::vector<core::FixItHint> guardFix(slang::BufferID buffer, const std::string& path, std::string_view text) {
    if (!text.empty() && text.back() == '\0') text.remove_suffix(1);
    std::string name = std::filesystem::path(path).filename().string();
    for (char& c : name) c = std::isalnum(static_cast<unsigned char>(c)) ? static_cast<char>(std::toupper(c)) : '_';
    std::string body(text);
    if (!body.empty() && body.back() != '\n') body += '\n';
    const slang::SourceRange all(slang::SourceLocation(buffer, 0),
                                 slang::SourceLocation(buffer, static_cast<uint32_t>(text.size())));
    return {{all, "`ifndef " + name + "\n`define " + name + "\n\n" + body + "\n`endif // " + name + "\n",
             "Wrap the file in an `ifndef " + name + " include guard", core::FixSafety::Unsafe}};
}

class IncludeGuard : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-013"; }
    std::string_view getName() const override { return "IncludeGuard"; }
    std::string_view getDescription() const override {
        return "Every included header file (.svh) must be wrapped in standard macro include guards "
               "(`ifndef / `define / `endif) to prevent circular definitions and compiler redefinition errors.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            auto firstToken = tree->root().getFirstToken();
            if (firstToken.isMissing()) continue;

            auto buffer = firstToken.location().buffer();
            std::string filePath = std::string(sm->getFullPath(buffer));

            if (filePath.ends_with(".svh")) {
                std::string_view sourceText = sm->getSourceText(buffer);

                bool hasIfndef = (sourceText.find("`ifndef") != std::string_view::npos);
                bool hasDefine = (sourceText.find("`define") != std::string_view::npos);
                bool hasEndif  = (sourceText.find("`endif") != std::string_view::npos);

                if (!hasIfndef || !hasDefine || !hasEndif) {
                    diags.addViolation(
                        "SV-STY-013",
                        "IncludeGuard",
                        "Header file '" + filePath +
                        "' is missing standard macro include guards (`ifndef / `define / `endif). "
                        "By convention, all header files (.svh) must be protected by include guards to prevent multiple inclusion errors.",
                        core::Severity::Error,
                        firstToken.location(),
                        guardFix(buffer, filePath, sourceText)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIncludeGuard() {
    return std::make_unique<style::IncludeGuard>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createIncludeGuard)
