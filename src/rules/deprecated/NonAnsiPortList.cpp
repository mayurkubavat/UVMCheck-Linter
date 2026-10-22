#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class NonAnsiPortVisitor : public engine::SyntaxVisitor<NonAnsiPortVisitor> {
public:
    explicit NonAnsiPortVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    // 1. Modules / Interfaces / Programs with non-ANSI port lists: module foo(a, b);
    void handle(const slang::syntax::NonAnsiPortListSyntax& portList) {
        diags.addViolation(
            "SV-DEP-006",
            "NonAnsiPortList",
            "Non-ANSI port list declaration is obsolete in IEEE 1800 § 13.3. "
            "Separate declaration of port names in header and types in body introduces redundant typing and syntax hazards. "
            "Use modern ANSI-style inline port declarations: 'module name(input logic [7:0] a, output logic b)'.",
            core::Severity::Info,
            portList.openParen.location()
        );
        visitDefault(portList);
    }

    // 2. Tasks and Functions with body port declarations: task foo; input a; ... endtask
    void handle(const slang::syntax::FunctionDeclarationSyntax& fn) {
        if (!fn.prototype->portList) {
            for (const auto* item : fn.items) {
                if (item && item->kind == slang::syntax::SyntaxKind::PortDeclaration) {
                    std::string routineName = fn.prototype->name->toString();
                    diags.addViolation(
                        "SV-DEP-006",
                        "NonAnsiPortList",
                        "Routine '" + routineName +
                        "' uses obsolete non-ANSI port declarations inside the routine body. "
                        "Declare argument directions and types directly in the routine header (e.g. 'task " +
                        routineName + "(input bit [7:0] data, output bit err)').",
                        core::Severity::Info,
                        item->getFirstToken().location()
                    );
                    break;
                }
            }
        }
        visitDefault(fn);
    }

private:
    core::DiagnosticEngine& diags;
};

class NonAnsiPortList : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-006"; }
    std::string_view getName() const override { return "NonAnsiPortList"; }
    std::string_view getDescription() const override {
        return "Disallow obsolete non-ANSI port list declarations. Enforces modern ANSI-style inline port lists.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        NonAnsiPortVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonAnsiPortList() {
    return std::make_unique<deprecated::NonAnsiPortList>();
}
}
