#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class ScoreboardLeftoverQueue : public Rule {
public:
    std::string_view getId() const override { return "UVM-SCB-002"; }
    std::string_view getName() const override { return "ScoreboardLeftoverQueue"; }
    std::string_view getDescription() const override {
        return "Scoreboard expected transaction queues must be checked for drain completeness in "
               "check_phase or report_phase. Missing drain checks allow dropped DUT packets to pass silently.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            std::string lowerName = toLower(std::string(cls->name));
            bool isScoreboard = (lowerName.find("scoreboard") != std::string::npos) ||
                                (lowerName.find("scb") != std::string::npos) ||
                                uvmModel.isDerivedFrom(*cls, "uvm_scoreboard");

            if (!isScoreboard) continue;

            // Collect expected queue/container properties
            std::vector<const slang::ast::ValueSymbol*> queueProps;
            const slang::ast::SubroutineSymbol* checkPhase = nullptr;
            const slang::ast::SubroutineSymbol* reportPhase = nullptr;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty || member.kind == slang::ast::SymbolKind::Variable) {
                    const auto& prop = member.as<slang::ast::ValueSymbol>();
                    const auto& type = prop.getType();
                    if (type.isQueue() || type.isAssociativeArray()) {
                        std::string propLower = toLower(std::string(prop.name));
                        // Identify queues named expected, exp, pred, predicted, item_q, etc.
                        if (propLower.find("exp") != std::string::npos ||
                            propLower.find("pred") != std::string::npos ||
                            propLower.find("queue") != std::string::npos ||
                            propLower.find("_q") != std::string::npos) {
                            queueProps.push_back(&prop);
                        }
                    }
                } else if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "check_phase") {
                        checkPhase = &sub;
                    } else if (sub.name == "report_phase") {
                        reportPhase = &sub;
                    }
                }
            }

            if (queueProps.empty()) continue;

            // Combine text of check_phase and report_phase, stripping line comments
            std::string checkPhaseText;
            if (checkPhase && checkPhase->getSyntax()) {
                checkPhaseText = std::string(checkPhase->getSyntax()->toString());
            }
            std::string reportPhaseText;
            if (reportPhase && reportPhase->getSyntax()) {
                reportPhaseText = std::string(reportPhase->getSyntax()->toString());
            }
            std::string rawText = checkPhaseText + "\n" + reportPhaseText;

            std::string cleanText;
            std::istringstream stream(rawText);
            std::string line;
            while (std::getline(stream, line)) {
                size_t lineCommentPos = line.find("//");
                if (lineCommentPos != std::string::npos) {
                    line = line.substr(0, lineCommentPos);
                }
                cleanText += line + "\n";
            }

            for (const auto* prop : queueProps) {
                std::string propName = std::string(prop->name);
                bool checkedInPhase = false;

                if (!cleanText.empty()) {
                    if (cleanText.find(propName + ".size") != std::string::npos ||
                        cleanText.find(propName + ".num") != std::string::npos ||
                        cleanText.find(propName + ".empty") != std::string::npos ||
                        cleanText.find(propName + " == 0") != std::string::npos) {
                        checkedInPhase = true;
                    }
                }

                if (!checkedInPhase) {
                    diags.addViolation(
                        "UVM-SCB-002",
                        "ScoreboardLeftoverQueue",
                        "Scoreboard class '" + std::string(cls->name) + "' declares expected transaction container '" +
                        propName + "', but neither 'check_phase' nor 'report_phase' verifies that the container is empty "
                        "at the end of simulation. If the DUT drops packets, simulation will complete with 0 errors. "
                        "Add 'if (" + propName + ".size() != 0) `uvm_error(\"DROPPED_PKT\", ...)' in check_phase().",
                        core::Severity::Error,
                        prop->location
                    );
                }
            }
        }
    }

private:
    static std::string toLower(std::string s) {
        for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        return s;
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createScoreboardLeftoverQueue() {
    return std::make_unique<uvm::ScoreboardLeftoverQueue>();
}
}
