#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class SilentDropVisitor : public slang::ast::ASTVisitor<SilentDropVisitor, slang::ast::VisitFlags::AllGood> {
public:
    SilentDropVisitor(std::string_view className, std::string_view methodName, core::DiagnosticEngine& diags)
        : className(className), methodName(methodName), diags(diags) {}

    void handle(const slang::ast::ConditionalStatement& cond) {
        if (!cond.conditions.empty()) {
            const auto* expr = cond.conditions[0].expr.get();
            if (expr && expr->syntax) {
                std::string condStr = std::string(expr->syntax->toString());
                // Look for checks indicating empty queue or missing expected item
                if (condStr.find(".size()") != std::string::npos ||
                    condStr.find(".num()") != std::string::npos ||
                    condStr.find(".empty()") != std::string::npos) {

                    if (cond.ifTrue.syntax) {
                        std::string bodyStr = std::string(cond.ifTrue.syntax->toString());
                        // If body returns without reporting an error
                        if (bodyStr.find("return") != std::string::npos) {
                            bool hasErrorOrAssert = (bodyStr.find("uvm_error") != std::string::npos) ||
                                                    (bodyStr.find("uvm_fatal") != std::string::npos) ||
                                                    (bodyStr.find("assert") != std::string::npos) ||
                                                    (bodyStr.find("error") != std::string::npos) ||
                                                    (bodyStr.find("fatal") != std::string::npos) ||
                                                    (bodyStr.find("drop_count") != std::string::npos);

                            if (!hasErrorOrAssert) {
                                diags.addViolation(
                                    "UVM-SCB-001",
                                    "ScoreboardSilentDrop",
                                    "Analysis export method '" + std::string(methodName) + "' in scoreboard class '" +
                                    std::string(className) + "' silently returns when expected queue is empty ('" +
                                    condStr + "') without logging an error. If the DUT generates spurious, "
                                    "misrouted, or unpredicted packets, this silent drop masks the hardware bug. "
                                    "Report an error with '`uvm_error(\"SPURIOUS_PKT\", ...)' before returning.",
                                    core::Severity::Error,
                                    cond.sourceRange.start()
                                );
                            }
                        }
                    }
                }
            }
        }
        visitDefault(cond);
    }

private:
    std::string_view className;
    std::string_view methodName;
    core::DiagnosticEngine& diags;
};

class ScoreboardSilentDrop : public Rule {
public:
    std::string_view getId() const override { return "UVM-SCB-001"; }
    std::string_view getName() const override { return "ScoreboardSilentDrop"; }
    std::string_view getDescription() const override {
        return "Detects silent returns in scoreboard write methods when expected queues are empty without "
               "logging an error. Spurious or unpredicted DUT packets are dropped, hiding hardware bugs.";
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

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name.starts_with("write")) {
                        SilentDropVisitor visitor(cls->name, sub.name, diags);
                        sub.getBody().visit(visitor);
                    }
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
std::unique_ptr<Rule> createScoreboardSilentDrop() {
    return std::make_unique<uvm::ScoreboardSilentDrop>();
}
}
