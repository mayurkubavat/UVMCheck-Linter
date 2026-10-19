#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class DelayFinder : public slang::syntax::SyntaxVisitor<DelayFinder> {
public:
    const slang::syntax::SyntaxNode* firstDelayNode = nullptr;

    void handle(const slang::syntax::DelaySyntax& delay) {
        if (!firstDelayNode) {
            firstDelayNode = &delay;
        }
        visitDefault(delay);
    }

    void handle(const slang::syntax::TimingControlStatementSyntax& stmt) {
        if (!firstDelayNode) {
            firstDelayNode = &stmt;
        }
        visitDefault(stmt);
    }
};

static bool hasActualTimescaleDirective(std::string_view sourceText) {
    size_t pos = 0;
    while ((pos = sourceText.find("`timescale", pos)) != std::string_view::npos) {
        size_t lineStart = sourceText.rfind('\n', pos);
        if (lineStart == std::string_view::npos) lineStart = 0;
        else lineStart++;

        std::string_view prefix = sourceText.substr(lineStart, pos - lineStart);
        if (prefix.find("//") == std::string_view::npos &&
            prefix.find("/*") == std::string_view::npos) {
            return true;
        }
        pos += 10;
    }

    // Also check timeunit keyword
    pos = 0;
    while ((pos = sourceText.find("timeunit", pos)) != std::string_view::npos) {
        size_t lineStart = sourceText.rfind('\n', pos);
        if (lineStart == std::string_view::npos) lineStart = 0;
        else lineStart++;

        std::string_view prefix = sourceText.substr(lineStart, pos - lineStart);
        if (prefix.find("//") == std::string_view::npos &&
            prefix.find("/*") == std::string_view::npos) {
            return true;
        }
        pos += 8;
    }

    return false;
}

class MissingTimescaleInModule : public Rule {
public:
    std::string_view getId() const override { return "SV-TIM-002"; }
    std::string_view getName() const override { return "MissingTimescaleInModule"; }
    std::string_view getDescription() const override {
        return "Flags files and modules containing procedural delays (#) that lack an explicit `timescale or timeunit declaration.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (!tree) continue;

            for (auto buffer : tree->getSourceBufferIds()) {
                if (sm->isIncludedFileLoc(slang::SourceLocation(buffer, 0))) continue;
                std::string_view sourceText = sm->getSourceText(buffer);

                bool hasTimescale = hasActualTimescaleDirective(sourceText);

                if (!hasTimescale) {
                    DelayFinder finder;
                    tree->root().visit(finder);

                    if (finder.firstDelayNode) {
                        diags.addViolation(
                            "SV-TIM-002",
                            "MissingTimescaleInModule",
                            "File contains procedural timing controls ('#delay') but omits a '`timescale' or 'timeunit' directive. Per IEEE 1800 § 3.14, omitting explicit timescale specifications causes simulators to adopt implementation-dependent default time units, introducing unpredictable timing skew and race conditions across VIP components.",
                            core::Severity::Warning,
                            finder.firstDelayNode->getFirstToken().location()
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMissingTimescaleInModule() {
    return std::make_unique<correctness::MissingTimescaleInModule>();
}
} // namespace uvmcheck::rules
