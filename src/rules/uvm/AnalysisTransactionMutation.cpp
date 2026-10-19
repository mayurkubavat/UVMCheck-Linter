#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/expressions/MiscExpressions.h"

namespace uvmcheck::rules::uvm {

class AnalysisTransactionMutation : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-005"; }
    std::string_view getName() const override { return "AnalysisTransactionMutation"; }
    std::string_view getDescription() const override {
        return "Detects in-place modification of transactions received via analysis write() methods. "
               "Because analysis ports broadcast by reference, modifying transaction fields directly "
               "causes silent data corruption across other concurrent subscribers and scoreboards.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        class WriteMethodMutationVisitor : public slang::ast::ASTVisitor<WriteMethodMutationVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            const uvmcheck::uvm::UvmModel& model;
            core::DiagnosticEngine& diags;
            std::string currentParamName;
            bool insideWriteMethod = false;

            WriteMethodMutationVisitor(const uvmcheck::uvm::UvmModel& m, core::DiagnosticEngine& d)
                : model(m), diags(d) {}

            void handle(const slang::ast::SubroutineSymbol& sub) {
                if (sub.name == "write" && sub.subroutineKind == slang::ast::SubroutineKind::Function) {
                    auto args = sub.getArguments();
                    if (!args.empty() && args[0]) {
                        std::string prevParam = currentParamName;
                        bool prevInside = insideWriteMethod;

                        currentParamName = std::string(args[0]->name);
                        insideWriteMethod = true;

                        visitDefault(sub);

                        currentParamName = prevParam;
                        insideWriteMethod = prevInside;
                        return;
                    }
                }
                visitDefault(sub);
            }

            void handle(const slang::ast::AssignmentExpression& assign) {
                if (insideWriteMethod && !currentParamName.empty()) {
                    const auto& left = assign.left();
                    std::string lhsText;
                    if (left.syntax) {
                        lhsText = std::string(left.syntax->toString());
                    }

                    // Check if LHS starts with paramName. (e.g. t.data = ...)
                    std::string targetPrefix = currentParamName + ".";
                    if (lhsText.rfind(targetPrefix, 0) == 0 || lhsText.find(" " + targetPrefix) != std::string::npos) {
                        diags.addViolation(
                            "UVM-TLM-005",
                            "AnalysisTransactionMutation",
                            "Direct in-place modification of analysis transaction '" + currentParamName +
                            "' detected in write() method ('" + lhsText + "'). "
                            "Clone or copy the transaction (e.g. $cast(local_tr, " + currentParamName +
                            ".clone())) before modifying its contents to prevent race conditions across broadcast subscribers.",
                            core::Severity::Warning,
                            assign.sourceRange.start()
                        );
                    }
                }
                visitDefault(assign);
            }
        };

        WriteMethodMutationVisitor visitor(uvmModel, diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAnalysisTransactionMutation() {
    return std::make_unique<uvm::AnalysisTransactionMutation>();
}
}
