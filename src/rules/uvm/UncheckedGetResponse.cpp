#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::uvm {

class GetResponseVisitor : public slang::syntax::SyntaxVisitor<GetResponseVisitor> {
public:
    explicit GetResponseVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string prevClass = currentClass;
        if (cls.name.valid()) {
            currentClass = std::string(cls.name.valueText());
        }
        visitDefault(cls);
        currentClass = prevClass;
    }

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string_view callee = inv.left->getLastToken().valueText();
            if (callee == "get_response") {
                std::string target = currentClass.empty() ? "sequence" : currentClass;
                diags.addViolation(
                    "UVM-SEQ-006",
                    "UncheckedGetResponse",
                    "UVM sequence class '" + target +
                    "' calls 'get_response()'. In UVM, calling get_response() blocks the sequence thread until the driver provides a response via 'seq_item_port.item_done(rsp)' or 'put_response(rsp)'. If the driver is not implemented to return response items, the sequence will hang indefinitely.",
                    core::Severity::Warning,
                    inv.sourceRange().start()
                );
            }
        }
        visitDefault(inv);
    }

private:
    std::string currentClass;
    core::DiagnosticEngine& diags;
};

class UncheckedGetResponse : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-006"; }
    std::string_view getName() const override { return "UncheckedGetResponse"; }
    std::string_view getDescription() const override {
        return "Flags calls to get_response() in UVM sequences. Calling get_response() requires the driver to "
               "actively provide response items; otherwise, the sequencer response FIFO remains empty and deadlocks simulation.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        GetResponseVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUncheckedGetResponse() {
    return std::make_unique<uvm::UncheckedGetResponse>();
}
}
