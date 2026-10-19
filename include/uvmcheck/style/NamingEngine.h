#pragma once

#include <string>
#include <string_view>
#include <vector>
#include <unordered_map>
#include <regex>
#include <optional>
#include "slang/ast/Compilation.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::style {

enum class CasingStyle {
    None,
    SnakeCase,     // e.g. packet_count
    PascalCase,    // e.g. AxiMasterDriver
    CamelCase,     // e.g. packetCount
    AllCaps        // e.g. MAX_BUFFER_SIZE
};

struct NamingTargetSpec {
    std::string ruleId;         // e.g. "SVTB.27.1" / "SV-NAM-001"
    std::string targetKind;     // "Package", "Interface", "Covergroup", "UvmDriver", etc.
    std::string regexPattern;   // compiled regex string
    std::string prefix;         // required prefix
    std::string suffix;         // required suffix
    CasingStyle casing = CasingStyle::None;
    core::Severity severity = core::Severity::Warning;
    std::string customMessage;  // optional explanation
    bool enabled = true;
};

class NamingEngine {
public:
    NamingEngine();

    /// Set or update the naming specification for a specific target kind
    void setRule(const std::string& targetKind, const NamingTargetSpec& spec);

    /// Check if a rule exists for a given target kind
    bool hasRule(const std::string& targetKind) const;

    /// Retrieve the rule specification for a target kind
    const NamingTargetSpec* getRule(const std::string& targetKind) const;

    /// Validate a given identifier against a target kind specification
    bool validate(const std::string& targetKind,
                  std::string_view name,
                  std::string& outViolationReason,
                  std::string& outSuggestedName) const;

    /// Execute comprehensive naming checks across the entire compilation AST and CST
    void run(slang::ast::Compilation& compilation,
             const uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) const;

    /// Helper to test casing styles
    static bool isSnakeCase(std::string_view name);
    static bool isPascalCase(std::string_view name);
    static bool isCamelCase(std::string_view name);
    static bool isAllCaps(std::string_view name);

    /// Helper to convert casing
    static std::string toPascalCase(std::string_view name);
    static std::string toSnakeCase(std::string_view name);
    static std::string toAllCaps(std::string_view name);

    /// Reset all naming targets to industry defaults (IEEE 1800 / UVM standard)
    void loadDefaultRules();

private:
    std::unordered_map<std::string, NamingTargetSpec> rules;
    mutable std::unordered_map<std::string, std::optional<std::regex>> compiledRegexCache;

    bool checkPattern(const NamingTargetSpec& spec, std::string_view name) const;
};

} // namespace uvmcheck::style
