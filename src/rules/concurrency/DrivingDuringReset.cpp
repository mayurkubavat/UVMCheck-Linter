#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::concurrency {

class DrivingDuringResetVisitor : public slang::syntax::SyntaxVisitor<DrivingDuringResetVisitor> {
public:
    explicit DrivingDuringResetVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& routine) {
        if (routine.prototype &&
            routine.prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword) {
            std::string taskText = std::string(routine.toString());

            // Check if task drives interface signals (e.g. vif.valid, intf.valid <= ...)
            bool drivesSignals = (taskText.find("vif.") != std::string::npos ||
                                  taskText.find("intf.") != std::string::npos) &&
                                 (taskText.find("<=") != std::string::npos);

            if (drivesSignals) {
                // Strip comments so explanatory comments like "// VIOLATION: missing reset" do not act as guards
                std::string cleanText = taskText;
                size_t commentPos;
                while ((commentPos = cleanText.find("//")) != std::string::npos) {
                    size_t eol = cleanText.find('\n', commentPos);
                    if (eol == std::string::npos) eol = cleanText.size();
                    cleanText.erase(commentPos, eol - commentPos);
                }

                std::string lowerTask = toLower(cleanText);
                bool hasResetGuard = (lowerTask.find("rst") != std::string::npos) ||
                                     (lowerTask.find("reset") != std::string::npos);

                if (!hasResetGuard) {
                    std::string taskName = std::string(routine.prototype->name->getFirstToken().valueText());
                    diags.addViolation(
                        "SV-DRV-001",
                        "DrivingDuringReset",
                        "Task '" + taskName + "' drives virtual interface signals without an active reset check or synchronization guard. "
                        "Driving protocol control signals while hardware reset is asserted causes bus protocol violations "
                        "and synchronizer state corruption. Guard signal driving with 'wait(vif.rst_n == 1)' or 'if (!vif.rst_n)'.",
                        core::Severity::Warning,
                        routine.prototype->name->sourceRange().start()
                    );
                }
            }
        }
        visitDefault(routine);
    }

private:
    static std::string toLower(std::string s) {
        for (char& c : s) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        return s;
    }

    core::DiagnosticEngine& diags;
};

class DrivingDuringReset : public Rule {
public:
    std::string_view getId() const override { return "SV-DRV-001"; }
    std::string_view getName() const override { return "DrivingDuringReset"; }
    std::string_view getDescription() const override {
        return "Disallow driving virtual interface control signals without an active reset check or synchronization guard. "
               "Asserting handshake signals during reset violates physical bus protocols and corrupts hardware state.";
    }
    std::string_view getCategory() const override { return "RTL_TB_Bridge"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        DrivingDuringResetVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDrivingDuringReset() {
    return std::make_unique<concurrency::DrivingDuringReset>();
}
}
