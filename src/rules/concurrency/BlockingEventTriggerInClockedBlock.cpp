#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class EventTriggerVisitor : public slang::syntax::SyntaxVisitor<EventTriggerVisitor> {
public:
    explicit EventTriggerVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::EventTriggerStatementSyntax& trigger) {
        if (trigger.kind == slang::syntax::SyntaxKind::BlockingEventTriggerStatement) {
            bool inClockedContext = false;
            for (const auto* curr = trigger.parent.get(); curr != nullptr; curr = curr->parent.get()) {
                if (curr->kind == slang::syntax::SyntaxKind::AlwaysFFBlock) {
                    inClockedContext = true;
                    break;
                }
                if (curr->kind == slang::syntax::SyntaxKind::TimingControlStatement ||
                    curr->kind == slang::syntax::SyntaxKind::AlwaysBlock) {
                    std::string s = std::string(curr->toString());
                    if (s.find("posedge") != std::string::npos ||
                        s.find("negedge") != std::string::npos) {
                        inClockedContext = true;
                        break;
                    }
                }
            }

            if (inClockedContext) {
                std::string evName = std::string(trigger.name ? trigger.name->toString() : "event");
                diags.addViolation(
                    "SV-CON-013",
                    "BlockingEventTriggerInClockedBlock",
                    "Blocking event trigger '-> " + evName +
                    "' is invoked within a sequential clocked process. In SystemVerilog (IEEE 1800 § 15.5 and "
                    "IEEE 1800 § 15.5), clocked processes should trigger events using "
                    "non-blocking trigger ('->> " + evName + "') to schedule notification in the Nonblocking Events "
                    "region, eliminating zero-time race conditions with concurrent listener threads.",
                    core::Severity::Warning,
                    trigger.trigger.location()
                );
            }
        }
        visitDefault(trigger);
    }

private:
    core::DiagnosticEngine& diags;
};

class BlockingEventTriggerInClockedBlock : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-013"; }
    std::string_view getName() const override { return "BlockingEventTriggerInClockedBlock"; }
    std::string_view getDescription() const override {
        return "Disallows blocking event triggers (-> ev) inside clocked blocks; non-blocking triggers (->> ev) should be used instead.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EventTriggerVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBlockingEventTriggerInClockedBlock() {
    return std::make_unique<concurrency::BlockingEventTriggerInClockedBlock>();
}
} // namespace uvmcheck::rules
