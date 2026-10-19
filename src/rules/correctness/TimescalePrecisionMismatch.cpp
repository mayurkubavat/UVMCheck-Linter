#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxTree.h"
#include <string>
#include <string_view>

namespace uvmcheck::rules::correctness {

static int parseTimeUnitExponent(std::string_view s) {
    while (!s.empty() && (s.front() == ' ' || s.front() == '\t')) s.remove_prefix(1);
    while (!s.empty() && (s.back() == ' ' || s.back() == '\t' || s.back() == '\r' || s.back() == '\n' || s.back() == ';')) s.remove_suffix(1);

    int logMult = 0;
    if (s.starts_with("100")) { logMult = 2; s.remove_prefix(3); }
    else if (s.starts_with("10")) { logMult = 1; s.remove_prefix(2); }
    else if (s.starts_with("1")) { logMult = 0; s.remove_prefix(1); }
    else return 999;

    while (!s.empty() && (s.front() == ' ' || s.front() == '\t')) s.remove_prefix(1);

    int baseExp = 0;
    if (s == "s") baseExp = 0;
    else if (s == "ms") baseExp = -3;
    else if (s == "us") baseExp = -6;
    else if (s == "ns") baseExp = -9;
    else if (s == "ps") baseExp = -12;
    else if (s == "fs") baseExp = -15;
    else return 999;

    return baseExp + logMult;
}

class TimescalePrecisionMismatch : public Rule {
public:
    std::string_view getId() const override { return "SV-TIM-001"; }
    std::string_view getName() const override { return "TimescalePrecisionMismatch"; }
    std::string_view getDescription() const override {
        return "Flags `timescale directives where the time precision is coarser than the time unit (IEEE 1800 § 3.14.2).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;

        for (const auto& tree : compilation.getSyntaxTrees()) {
            for (auto buffer : tree->getSourceBufferIds()) {
                if (sm->isIncludedFileLoc(slang::SourceLocation(buffer, 0))) continue;
                std::string_view sourceText = sm->getSourceText(buffer);

            size_t pos = 0;
            while ((pos = sourceText.find("`timescale", pos)) != std::string_view::npos) {
                if (pos == 0 || sourceText[pos - 1] == '\n' || sourceText[pos - 1] == ' ' || sourceText[pos - 1] == '\t') {
                    size_t lineEnd = sourceText.find_first_of("\r\n", pos);
                    if (lineEnd == std::string_view::npos) lineEnd = sourceText.size();

                    std::string_view directiveLine = sourceText.substr(pos + 10, lineEnd - (pos + 10));
                    size_t slashPos = directiveLine.find('/');
                    if (slashPos != std::string_view::npos) {
                        std::string_view unitStr = directiveLine.substr(0, slashPos);
                        std::string_view precStr = directiveLine.substr(slashPos + 1);

                        int unitExp = parseTimeUnitExponent(unitStr);
                        int precExp = parseTimeUnitExponent(precStr);

                        if (unitExp != 999 && precExp != 999 && precExp > unitExp) {
                            slang::SourceLocation loc(buffer, static_cast<uint32_t>(pos));
                            std::string cleanUnit = std::string(unitStr);
                            cleanUnit.erase(cleanUnit.find_last_not_of(" \t") + 1);
                            cleanUnit.erase(0, cleanUnit.find_first_not_of(" \t"));
                            std::string cleanPrec = std::string(precStr);
                            cleanPrec.erase(cleanPrec.find_last_not_of(" \t\r\n;") + 1);
                            cleanPrec.erase(0, cleanPrec.find_first_not_of(" \t"));

                            diags.addViolation(
                                "SV-TIM-001",
                                "TimescalePrecisionMismatch",
                                "Timescale directive specifies a time precision ('" + cleanPrec +
                                "') coarser than the time unit ('" + cleanUnit +
                                "'). Per IEEE 1800 § 3.14.2, time precision must be smaller than or equal to the time unit to prevent simulation rounding errors.",
                                core::Severity::Error,
                                loc
                            );
                        }
                    }
                }
                pos += 10;
            }
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTimescalePrecisionMismatch() {
    return std::make_unique<correctness::TimescalePrecisionMismatch>();
}
}
