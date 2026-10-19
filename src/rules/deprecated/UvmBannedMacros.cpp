#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class UvmBannedMacrosScanner {
public:
    static void run(slang::ast::Compilation& compilation, core::DiagnosticEngine& diags, bool checkField = true, bool checkDo = true) {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            for (auto buffer : tree->getSourceBufferIds()) {
                if (sm->isIncludedFileLoc(slang::SourceLocation(buffer, 0))) continue;
                std::string_view sourceText = sm->getSourceText(buffer);

                size_t offset = 0;
                while (offset < sourceText.size()) {
                    size_t nextNewline = sourceText.find('\n', offset);
                    if (nextNewline == std::string_view::npos) {
                        nextNewline = sourceText.size();
                    }

                    std::string_view line = sourceText.substr(offset, nextNewline - offset);

                    // Trim leading whitespace
                    size_t firstNonSpace = line.find_first_not_of(" \t");
                    if (firstNonSpace != std::string_view::npos) {
                        std::string_view trimmed = line.substr(firstNonSpace);

                        // Skip comment lines
                        if (!trimmed.starts_with("//") && !trimmed.starts_with("/*")) {
                            // 1. UVM-BAN-001: uvm_field_* macros & uvm_*_utils_begin
                            if (checkField) {
                                size_t fieldPos = line.find("`uvm_field_");
                                size_t utilsBeginPos = line.find("`uvm_object_utils_begin");
                                if (utilsBeginPos == std::string_view::npos) {
                                    utilsBeginPos = line.find("`uvm_component_utils_begin");
                                }

                                if (fieldPos != std::string_view::npos || utilsBeginPos != std::string_view::npos) {
                                    size_t hitPos = (fieldPos != std::string_view::npos) ? fieldPos : utilsBeginPos;
                                    diags.addViolation(
                                        "UVM-BAN-001",
                                        "UvmBannedFieldMacros",
                                        "The `uvm_field_* macro suite is banned in high-performance verification environments (Cliff Cummings / Verification Academy). "
                                        "Field macros introduce severe simulation overhead (3x-10x execution drag), uncontrollable deep copying, "
                                        "and opaque compile errors. Implement explicit 'do_copy', 'do_compare', and 'do_print' methods instead.",
                                        core::Severity::Warning,
                                        slang::SourceLocation(buffer, static_cast<uint32_t>(offset + hitPos))
                                    );
                                }
                            }

                            // 2. UVM-BAN-002: `uvm_do macros
                            if (checkDo) {
                                size_t doPos = line.find("`uvm_do");
                                if (doPos != std::string_view::npos) {
                                    // Check that it's actually `uvm_do or `uvm_do_with / `uvm_do_on
                                    std::string_view rest = line.substr(doPos + 7);
                                    if (rest.empty() || rest.starts_with("(") || rest.starts_with(" ") ||
                                        rest.starts_with("_with") || rest.starts_with("_on") || rest.starts_with("_pri")) {
                                        diags.addViolation(
                                            "UVM-BAN-002",
                                            "UvmBannedUvmDoMacros",
                                            "Macro '`uvm_do' is discouraged in modern sequence development. "
                                            "It obscures the sequence-driver protocol handshake and conceals transaction randomization. "
                                            "Use explicit driver handshake: 'start_item(req)' -> 'assert(req.randomize())' -> 'finish_item(req)'.",
                                            core::Severity::Warning,
                                            slang::SourceLocation(buffer, static_cast<uint32_t>(offset + doPos))
                                        );
                                    }
                                }
                            }
                        }
                    }

                    offset = nextNewline + 1;
                }
            }
        }
    }
};

class UvmBannedFieldMacros : public Rule {
public:
    std::string_view getId() const override { return "UVM-BAN-001"; }
    std::string_view getName() const override { return "UvmBannedFieldMacros"; }
    std::string_view getDescription() const override {
        return "Bans the `uvm_field_* macro suite. Mandates manual do_copy, do_compare, and do_print methods "
               "to eliminate severe simulation performance overhead and uncontrollable deep copy bugs.";
    }
    std::string_view getCategory() const override { return "AntiPattern"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmBannedMacrosScanner::run(compilation, diags, true, false);
    }
};

class UvmBannedUvmDoMacros : public Rule {
public:
    std::string_view getId() const override { return "UVM-BAN-002"; }
    std::string_view getName() const override { return "UvmBannedUvmDoMacros"; }
    std::string_view getDescription() const override {
        return "Discourages black-box `uvm_do macros in sequence bodies. "
               "Mandates explicit start_item / finish_item transaction handshakes.";
    }
    std::string_view getCategory() const override { return "AntiPattern"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmBannedMacrosScanner::run(compilation, diags, false, true);
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmBannedFieldMacros() {
    return std::make_unique<deprecated::UvmBannedFieldMacros>();
}
std::unique_ptr<Rule> createUvmBannedUvmDoMacros() {
    return std::make_unique<deprecated::UvmBannedUvmDoMacros>();
}
}
