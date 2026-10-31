#include "uvmcheck/rules/CompilerDiagnostics.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include <atomic>
#include <optional>

#include "slang/diagnostics/DiagnosticEngine.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::rules {

namespace {
std::atomic<const slang::DiagnosticEngine*> compilerEngine{nullptr};
} // namespace

void setCompilerDiagnosticEngine(const slang::DiagnosticEngine* engine) {
    compilerEngine.store(engine);
}

namespace compiler {

/// Reports the compiler's own errors and warnings as findings, so they reach
/// JSON, SARIF and the editor, and can be waived or baselined like any other
/// finding. Severity follows the compiler (with -W options and
/// `pragma diagnostic`); notes become related locations.
class CompilerDiagnostics : public Rule {
public:
    std::string_view getId() const override { return "SV-CMP-001"; }
    std::string_view getName() const override { return "CompilerError"; }
    std::string_view getDescription() const override {
        return "Reports the SystemVerilog compiler's errors (syntax, elaboration and type errors) as findings, "
               "so they appear in every report and in the editor next to the lint findings.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    std::vector<SubRule> getSubRules() const override {
        return {{"SV-CMP-002", "CompilerWarning",
                 "Reports the SystemVerilog compiler's warnings as findings (controlled with -W options).",
                 core::Severity::Warning}};
    }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& compilation = ctx.compilation();
        auto& diags = ctx.diags();
        const auto* sm = compilation.getSourceManager();
        if (!sm) co_return;

        std::optional<slang::DiagnosticEngine> fallback;
        const slang::DiagnosticEngine* engine = compilerEngine.load();
        if (!engine) engine = &fallback.emplace(*sm);
        const auto& classifier = *engine;

        for (const auto& diag : compilation.getAllDiagnostics()) {
            // Design-wide diagnostics (e.g. no top module) have no file to point at.
            if (!diag.location.valid() || sm->getFileName(sm->getFullyOriginalLoc(diag.location)).empty())
                continue;
            const auto severity = classifier.getSeverity(diag.code, diag.location);
            const char* id = nullptr;
            const char* name = nullptr;
            core::Severity sev = core::Severity::Warning;
            switch (severity) {
                case slang::DiagnosticSeverity::Error:
                case slang::DiagnosticSeverity::Fatal:
                    id = "SV-CMP-001", name = "CompilerError", sev = core::Severity::Error;
                    break;
                case slang::DiagnosticSeverity::Warning:
                    id = "SV-CMP-002", name = "CompilerWarning", sev = core::Severity::Warning;
                    break;
                default:
                    continue;
            }

            std::string message = classifier.formatMessage(diag);
            if (auto option = classifier.getOptionName(diag.code); !option.empty())
                message += " [-W" + std::string(option) + "]";

            auto* v = diags.addViolation(id, name, message, sev, diag.location);
            if (!v) continue;
            for (const auto& range : diag.ranges) {
                if (range.start() == diag.location) {
                    diags.setRange(v, range);
                    break;
                }
            }
            for (const auto& note : diag.notes) {
                if (note.location.valid())
                    diags.addRelated(v, note.location, classifier.formatMessage(note));
            }
        }
    }
};

} // namespace compiler

std::unique_ptr<Rule> createCompilerDiagnostics() {
    return std::make_unique<compiler::CompilerDiagnostics>();
}

} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createCompilerDiagnostics)
