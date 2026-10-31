#pragma once

#include <string_view>

namespace slang {
class DiagnosticEngine;
}

namespace uvmcheck::rules {

/// The compiler front end's diagnostic engine (with its -W options and
/// compatibility settings). SV-CMP-001/002 classify and word the compiler's
/// diagnostics through it, so the findings match what the compiler reports.
/// Without one, slang's defaults apply.
void setCompilerDiagnosticEngine(const slang::DiagnosticEngine* engine);

/// Registers an engine for the lifetime of the scope.
class CompilerDiagnosticEngineScope {
public:
    explicit CompilerDiagnosticEngineScope(const slang::DiagnosticEngine& engine) {
        setCompilerDiagnosticEngine(&engine);
    }
    ~CompilerDiagnosticEngineScope() { setCompilerDiagnosticEngine(nullptr); }
    CompilerDiagnosticEngineScope(const CompilerDiagnosticEngineScope&) = delete;
    CompilerDiagnosticEngineScope& operator=(const CompilerDiagnosticEngineScope&) = delete;
};

/// True when a finding with this rule id is one of the compiler's own diagnostics.
inline bool isCompilerDiagnosticRule(std::string_view id) {
    return id == "SV-CMP-001" || id == "SV-CMP-002";
}

} // namespace uvmcheck::rules
