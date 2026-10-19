#include "uvmcheck/style/StyleEngine.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"
#include <iostream>
#include <sstream>
#include <filesystem>

namespace uvmcheck::style {

StyleEngine::StyleEngine() {}

StyleEngine::StyleEngine(const StyleConfiguration& config) : config(config) {}

void StyleEngine::inspectTextualStyle(const slang::syntax::SyntaxTree& tree,
                                     core::DiagnosticEngine& diags) const {
    auto startLoc = tree.root().sourceRange().start();
    if (!startLoc.valid()) return;
    auto buffer = startLoc.buffer();
    if (!buffer) return;

    std::string_view rawText = tree.sourceManager().getSourceText(buffer);
    if (rawText.empty()) return;

    std::string filePath = std::string(tree.sourceManager().getRawFileName(buffer));
    if (filePath.empty()) return;

    std::istringstream stream{std::string(rawText)};
    std::string line;
    size_t lineNum = 1;
    size_t fileOffset = 0;
    bool foundHeader = false;
    std::optional<std::regex> headerRegex;

    if (config.requireFileHeader && !config.fileHeaderPattern.empty()) {
        try {
            headerRegex = std::regex(config.fileHeaderPattern, std::regex::icase);
        } catch (const std::regex_error&) {
            headerRegex = std::nullopt;
        }
    }

    while (std::getline(stream, line)) {
        // Strip carriage return if CRLF
        if (!line.empty() && line.back() == '\r') {
            line.pop_back();
        }

        // Check 1: Tabs Prohibited (SVTB.1.1.1)
        if (!config.allowTabs) {
            size_t tabPos = line.find('\t');
            if (tabPos != std::string::npos) {
                slang::SourceLocation loc(tree.root().sourceRange().start().buffer(), fileOffset + tabPos);
                std::vector<core::FixItHint> fixIts;
                fixIts.push_back({
                    slang::SourceRange(loc, slang::SourceLocation(loc.buffer(), loc.offset() + 1)),
                    "    ",
                    "Replace tab character with 4 spaces"
                });

                diags.addViolation(
                    "SVTB.1.1.1",
                    "TabCharactersProhibited",
                    "Literal tab character found at column " + std::to_string(tabPos + 1) + ". "
                    "In SystemVerilog style guidelines, tab characters must be replaced with spaces "
                    "to prevent inconsistent rendering across editors and command-line viewers.",
                    core::Severity::Warning,
                    loc,
                    fixIts
                );
            }
        }

        // Check 2: Trailing Whitespace (SVTB.1.1.2)
        if (!config.allowTrailingWhitespace && !line.empty()) {
            char lastChar = line.back();
            if (lastChar == ' ' || lastChar == '\t') {
                size_t trailingStart = line.find_last_not_of(" \t");
                size_t whitespaceOffset = (trailingStart == std::string::npos) ? 0 : trailingStart + 1;
                slang::SourceLocation loc(tree.root().sourceRange().start().buffer(), fileOffset + whitespaceOffset);
                std::vector<core::FixItHint> fixIts;
                fixIts.push_back({
                    slang::SourceRange(loc, slang::SourceLocation(loc.buffer(), fileOffset + line.length())),
                    "",
                    "Remove trailing whitespace"
                });

                diags.addViolation(
                    "SVTB.1.1.2",
                    "TrailingWhitespace",
                    "Line contains trailing whitespace characters at line end. "
                    "Remove trailing spaces/tabs to prevent noisy version control diffs.",
                    core::Severity::Warning,
                    loc,
                    fixIts
                );
            }
        }

        // Check 3: Maximum Line Length (SVTB.1.1.3)
        if (config.maxLineLength > 0 && line.length() > config.maxLineLength) {
            slang::SourceLocation loc(tree.root().sourceRange().start().buffer(), fileOffset + config.maxLineLength);
            diags.addViolation(
                "SVTB.1.1.3",
                "MaxLineLengthExceeded",
                "Line exceeds maximum length limit of " + std::to_string(config.maxLineLength) +
                " characters (current: " + std::to_string(line.length()) + "). Wrap long lines for readable side-by-side reviews.",
                core::Severity::Warning,
                loc
            );
        }

        // Check 4: Header pattern check in top lines
        if (config.requireFileHeader && headerRegex.has_value() && lineNum <= 20) {
            if (std::regex_search(line, *headerRegex)) {
                foundHeader = true;
            }
        }

        fileOffset += line.length() + 1; // +1 for newline
        lineNum++;
    }

    if (config.requireFileHeader && headerRegex.has_value() && !foundHeader) {
        slang::SourceLocation startLoc = tree.root().sourceRange().start();
        diags.addViolation(
            "SVTB.25.1.1",
            "MissingFileHeader",
            "File '" + filePath + "' lacks required standard corporate copyright/license header pattern ('" +
            config.fileHeaderPattern + "').",
            core::Severity::Warning,
            startLoc
        );
    }
}

class StructuralStyleVisitor : public slang::syntax::SyntaxVisitor<StructuralStyleVisitor> {
public:
    StructuralStyleVisitor(const StyleConfiguration& config,
                          const slang::SourceManager& sm,
                          core::DiagnosticEngine& diags)
        : config(config), sm(sm), diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        if (config.mandatoryMatchingEndLabel) {
            if (!cls.endBlockName) {
                diags.addViolation(
                    "SVTB.1.5",
                    "MandatoryMatchingEndLabel",
                    "Class '" + std::string(cls.name.valueText()) + "' definition lacks matching end label (expected 'endclass : " +
                    std::string(cls.name.valueText()) + "').",
                    core::Severity::Warning,
                    cls.endClass.location()
                );
            }
        }

        if (config.fileMatchesClassName) {
            auto startLoc = cls.sourceRange().start();
            if (startLoc.valid()) {
                std::string filePath = std::string(sm.getRawFileName(startLoc.buffer()));
                if (!filePath.empty()) {
                    std::filesystem::path p(filePath);
                    std::string stem = p.stem().string();
                    std::string className = std::string(cls.name.valueText());
                    if (stem != className) {
                        diags.addViolation(
                            "SVTB.2.1.1",
                            "FileMatchesClassName",
                            "File name '" + p.filename().string() + "' does not match class name '" + className +
                            "'. SystemVerilog style guidelines recommend naming files after the primary class declared within them.",
                            core::Severity::Warning,
                            cls.name.location()
                        );
                    }
                }
            }
        }

        visitDefault(cls);
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (config.mandatoryMatchingEndLabel && func.prototype && func.prototype->name) {
            std::string routineName = std::string(func.prototype->name->getLastToken().valueText());
            if (!routineName.empty() && !func.endBlockName) {
                bool isTask = (func.kind == slang::syntax::SyntaxKind::TaskDeclaration);
                std::string kw = isTask ? "endtask" : "endfunction";
                std::string kindStr = isTask ? "Task" : "Function";
                diags.addViolation(
                    "SVTB.1.5",
                    "MandatoryMatchingEndLabel",
                    kindStr + " '" + routineName + "' lacks matching end label (expected '" + kw + " : " + routineName + "').",
                    core::Severity::Warning,
                    func.end.location()
                );
            }
        }
        visitDefault(func);
    }

    void handle(const slang::syntax::ModuleDeclarationSyntax& mod) {
        if (config.mandatoryMatchingEndLabel && mod.kind == slang::syntax::SyntaxKind::InterfaceDeclaration) {
            std::string ifName = std::string(mod.header->name.valueText());
            if (!ifName.empty() && !mod.blockName) {
                diags.addViolation(
                    "SVTB.1.5",
                    "MandatoryMatchingEndLabel",
                    "Interface '" + ifName + "' lacks matching end label (expected 'endinterface : " + ifName + "').",
                    core::Severity::Warning,
                    mod.endmodule.location()
                );
            }
        }
        visitDefault(mod);
    }

private:
    const StyleConfiguration& config;
    [[maybe_unused]] const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
};

void StyleEngine::inspectStructuralStyle(const slang::syntax::SyntaxTree& tree,
                                        core::DiagnosticEngine& diags) const {
    StructuralStyleVisitor visitor(config, tree.sourceManager(), diags);
    tree.root().visit(visitor);
}

void StyleEngine::run(slang::ast::Compilation& compilation,
                     const uvm::UvmModel& /*uvmModel*/,
                     core::DiagnosticEngine& diags) const {
    for (const auto& tree : compilation.getSyntaxTrees()) {
        if (!tree) continue;
        inspectTextualStyle(*tree, diags);
        inspectStructuralStyle(*tree, diags);
    }
}

} // namespace uvmcheck::style
