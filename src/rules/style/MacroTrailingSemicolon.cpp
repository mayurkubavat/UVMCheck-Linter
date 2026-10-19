#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/parsing/PreprocessorMetadata.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules::style {

class MacroTrailingSemicolon : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-002"; }
    std::string_view getName() const override { return "MacroTrailingSemicolon"; }
    std::string_view getDescription() const override {
        return "Flags unnecessary semicolons following `uvm_* macro invocations that create "
               "empty statements in declarative or procedural scopes.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            auto macroRefs = tree->getMacroRefs();
            for (const auto& mref : macroRefs) {
                if (!mref.syntax) continue;

                std::string_view macroName;
                if (mref.definition) {
                    macroName = mref.definition->name.valueText();
                }

                // Check for UVM macros where trailing semicolons cause null declarations
                // or empty statements
                if (!macroName.starts_with("uvm_")) {
                    continue;
                }

                auto range = mref.syntax->sourceRange();
                auto endLoc = range.end();
                if (!sm->isFileLoc(endLoc)) continue;

                auto fileId = endLoc.buffer();
                auto text = sm->getSourceText(fileId);
                size_t offset = endLoc.offset();

                // Scan ahead on the same line past whitespace / tabs
                size_t cur = offset;
                while (cur < text.size() && (text[cur] == ' ' || text[cur] == '\t')) {
                    cur++;
                }

                if (cur < text.size() && text[cur] == ';') {
                    slang::SourceLocation semiLoc(fileId, static_cast<uint32_t>(cur));
                    diags.addViolation(
                        "SV-STY-002",
                        "MacroTrailingSemicolon",
                        "Unnecessary semicolon ';' following macro '`" + std::string(macroName) +
                        "'. In declarative and block scopes, trailing semicolons create empty statements.",
                        core::Severity::Warning,
                        semiLoc
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMacroTrailingSemicolon() {
    return std::make_unique<style::MacroTrailingSemicolon>();
}
}
