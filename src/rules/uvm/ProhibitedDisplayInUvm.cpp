#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/ClassSymbols.h"

namespace uvmcheck::rules::uvm {

class DisplayCallVisitor : public slang::ast::ASTVisitor<DisplayCallVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit DisplayCallVisitor(core::DiagnosticEngine& diags, std::string_view className)
        : diags(diags), className(className) {}

    void handle(const slang::ast::CallExpression& expr) {
        std::string_view subName = expr.getSubroutineName();
        if (subName == "$display" || subName == "$displayh" || subName == "$displayb" || subName == "$displayo" ||
            subName == "$write" || subName == "$writeh" || subName == "$writeb" || subName == "$writeo" ||
            subName == "$strobe" || subName == "$strobeh" || subName == "$strobeb" || subName == "$strobeo") {

            diags.addViolation(
                "UVM-LOG-002",
                "ProhibitedDisplayInUvm",
                "Direct system task '" + std::string(subName) + "' called in UVM class '" + std::string(className) +
                "'. Direct display tasks bypass UVM report catchers, severity filtering (+uvm_set_verbosity), and regression log routing. Use `uvm_info or `uvm_error instead.",
                core::Severity::Warning,
                expr.sourceRange.start(),
                {}
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view className;
};

class ProhibitedDisplayInUvm : public Rule {
public:
    std::string_view getId() const override { return "UVM-LOG-002"; }
    std::string_view getName() const override { return "ProhibitedDisplayInUvm"; }
    std::string_view getDescription() const override {
        return "Disallow direct $display/$write/$strobe calls inside UVM classes (components, objects, sequences). "
               "SystemVerilog display tasks bypass UVM reporting infrastructure, log redirection, and verbosity filtering.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            if (!classType) continue;
            if (!uvmModel.isDerivedFrom(*classType, "uvm_report_object") &&
                !uvmModel.isUvmComponent(*classType) &&
                !uvmModel.isUvmSequence(*classType) &&
                !uvmModel.isDerivedFrom(*classType, "uvm_object")) {
                continue;
            }

            DisplayCallVisitor visitor(diags, classType->name);
            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                sub.getBody().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProhibitedDisplayInUvm() {
    return std::make_unique<uvm::ProhibitedDisplayInUvm>();
}
}
