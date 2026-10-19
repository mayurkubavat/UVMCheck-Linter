#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::style {

class EndLabelVisitor : public slang::syntax::SyntaxVisitor<EndLabelVisitor> {
public:
    explicit EndLabelVisitor(core::DiagnosticEngine& diags, bool checkMissing = true, bool checkMismatch = true)
        : diags(diags), checkMissing(checkMissing), checkMismatch(checkMismatch) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& fn) {
        std::string routineName = fn.prototype->name->toString();
        // Remove whitespace
        routineName.erase(remove_if(routineName.begin(), routineName.end(), isspace), routineName.end());

        if (!fn.endBlockName) {
            if (checkMissing) {
                std::vector<core::FixItHint> fixIts;
                if (fn.end.range().start().valid()) {
                    fixIts.push_back({fn.end.range(), std::string(fn.end.valueText()) + " : " + routineName, "Add matching end label"});
                }
                diags.addViolation(
                    "SV-STY-005",
                    "MandatoryMatchingEndLabel",
                    "Missing explicit end label on routine '" + routineName +
                    "'. By convention, routines must terminate with a matching label ('endfunction : " +
                    routineName + "' or 'endtask : " + routineName + "').",
                    core::Severity::Info,
                    fn.end.location(),
                    std::move(fixIts)
                );
            }
        } else if (checkMismatch) {
            std::string labelName = std::string(fn.endBlockName->name.valueText());
            bool matches = (labelName == routineName) ||
                           (routineName.ends_with("::" + labelName));

            if (!matches) {
                std::vector<core::FixItHint> fixIts;
                if (fn.endBlockName->name.range().start().valid()) {
                    fixIts.push_back({fn.endBlockName->name.range(), routineName, "Fix mismatched end label"});
                }
                diags.addViolation(
                    "SV-STY-006",
                    "MismatchedEndLabel",
                    "Mismatched end label ':" + labelName + "' does not match routine name '" + routineName +
                    "'. Update the label to match the routine declaration.",
                    core::Severity::Warning,
                    fn.endBlockName->name.location(),
                    std::move(fixIts)
                );
            }
        }
        visitDefault(fn);
    }

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string clsName = std::string(cls.name.valueText());

        if (!cls.endBlockName) {
            if (checkMissing) {
                std::vector<core::FixItHint> fixIts;
                if (cls.endClass.range().start().valid()) {
                    fixIts.push_back({cls.endClass.range(), std::string(cls.endClass.valueText()) + " : " + clsName, "Add matching end label"});
                }
                diags.addViolation(
                    "SV-STY-005",
                    "MandatoryMatchingEndLabel",
                    "Missing explicit end label on class '" + clsName +
                    "'. By convention, classes must terminate with a matching label ('endclass : " +
                    clsName + "').",
                    core::Severity::Info,
                    cls.endClass.location(),
                    std::move(fixIts)
                );
            }
        } else if (checkMismatch) {
            std::string labelName = std::string(cls.endBlockName->name.valueText());
            bool matches = (labelName == clsName);

            if (!matches) {
                std::vector<core::FixItHint> fixIts;
                if (cls.endBlockName->name.range().start().valid()) {
                    fixIts.push_back({cls.endBlockName->name.range(), clsName, "Fix mismatched end label"});
                }
                diags.addViolation(
                    "SV-STY-006",
                    "MismatchedEndLabel",
                    "Mismatched end label ':" + labelName + "' does not match class name '" + clsName +
                    "'. Update the label to match the class declaration.",
                    core::Severity::Warning,
                    cls.endBlockName->name.location(),
                    std::move(fixIts)
                );
            }
        }
        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
    bool checkMissing;
    bool checkMismatch;
};

class MandatoryMatchingEndLabel : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-005"; }
    std::string_view getName() const override { return "MandatoryMatchingEndLabel"; }
    std::string_view getDescription() const override {
        return "Enforces explicit and matching end labels on functions, tasks, and classes (e.g., endfunction : name). "
               "Prevents scope delimiter ambiguity in large verification codebases.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        EndLabelVisitor visitor(diags, true, false);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

class MismatchedEndLabel : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-006"; }
    std::string_view getName() const override { return "MismatchedEndLabel"; }
    std::string_view getDescription() const override {
        return "Flags mismatched end labels on functions, tasks, and classes (e.g., function void foo(); ... endfunction : bar). "
               "Prevents scope delimiter confusion and copy-paste errors.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        EndLabelVisitor visitor(diags, false, true);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMandatoryMatchingEndLabel() {
    return std::make_unique<style::MandatoryMatchingEndLabel>();
}
std::unique_ptr<Rule> createMismatchedEndLabel() {
    return std::make_unique<style::MismatchedEndLabel>();
}
}
