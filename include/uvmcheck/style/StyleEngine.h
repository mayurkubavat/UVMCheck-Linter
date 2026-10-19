#pragma once

#include <string>
#include <string_view>
#include <vector>
#include <regex>
#include <optional>
#include "slang/ast/Compilation.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::style {

struct StyleConfiguration {
    size_t maxLineLength = 120;             // SVTB.1.1.3: Maximum characters per line (0 = disabled)
    bool allowTabs = false;                 // SVTB.1.1.1: Literal tab characters prohibited
    bool allowTrailingWhitespace = false;   // SVTB.1.1.2: Trailing whitespace prohibited
    bool oneStatementPerLine = true;        // SVTB.1.4.1: Exactly one statement per line
    bool mandatoryMatchingEndLabel = true;  // SVTB.1.5: End labels required on blocks/routines
    bool requireFileHeader = false;         // SVTB.25.1.1: Standard copyright/SPDX header
    std::string fileHeaderPattern = "";     // Regex pattern for required header content
    bool fileMatchesClassName = false;      // SVTB.2.1.1: File basename must match top-level class
};

class StyleEngine {
public:
    StyleEngine();
    explicit StyleEngine(const StyleConfiguration& config);

    /// Update style settings
    void setConfig(const StyleConfiguration& config) { this->config = config; }
    const StyleConfiguration& getConfig() const { return config; }

    /// Execute text-level and syntax-level style checks
    void run(slang::ast::Compilation& compilation,
             const uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) const;

private:
    StyleConfiguration config;

    void inspectTextualStyle(const slang::syntax::SyntaxTree& tree,
                             core::DiagnosticEngine& diags) const;

    void inspectStructuralStyle(const slang::syntax::SyntaxTree& tree,
                                core::DiagnosticEngine& diags) const;
};

} // namespace uvmcheck::style
