#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"

namespace uvmcheck::rules::uvm {

class ObjectionCounterVisitor : public slang::ast::ASTVisitor<ObjectionCounterVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::CallExpression& expr) {
        std::string_view name = expr.getSubroutineName();
        if (name == "raise_objection") {
            raiseCount++;
            if (!firstRaiseLoc.valid()) {
                firstRaiseLoc = expr.sourceRange.start();
            }
        } else if (name == "drop_objection") {
            dropCount++;
            if (!firstDropLoc.valid()) {
                firstDropLoc = expr.sourceRange.start();
            }
        }
        visitDefault(expr);
    }

    size_t raiseCount = 0;
    size_t dropCount = 0;
    slang::SourceLocation firstRaiseLoc;
    slang::SourceLocation firstDropLoc;
};

class UvmUnpairedObjection : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBJ-001"; }
    std::string_view getName() const override { return "UvmUnpairedObjection"; }
    std::string_view getDescription() const override {
        return "Every phase.raise_objection() call must have a corresponding phase.drop_objection() "
               "within the same execution context to prevent simulation hangs or fatal errors.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            bool isComp = uvmModel.isUvmComponent(*cls);
            bool isSeq = uvmModel.isUvmSequence(*cls);

            if (!isComp && !isSeq) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::Subroutine) continue;

                const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                ObjectionCounterVisitor counter;
                sub.getBody().visit(counter);

                // Case 1: Raised but never dropped in this subroutine
                if (counter.raiseCount > 0 && counter.dropCount == 0) {
                    diags.addViolation(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        "Method '" + std::string(sub.name) + "' in class '" + std::string(cls->name) +
                        "' calls 'raise_objection' but has no corresponding 'drop_objection'. "
                        "This un-dropped objection will cause the simulation to hang indefinitely.",
                        core::Severity::Error,
                        counter.firstRaiseLoc
                    );
                }
                // Case 2: Dropped but never raised in this subroutine
                else if (counter.dropCount > 0 && counter.raiseCount == 0) {
                    diags.addViolation(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        "Method '" + std::string(sub.name) + "' in class '" + std::string(cls->name) +
                        "' calls 'drop_objection' without any prior 'raise_objection' in the same scope. "
                        "Dropping unraised objections leads to UVM runtime fatal errors.",
                        core::Severity::Error,
                        counter.firstDropLoc
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmUnpairedObjection() {
    return std::make_unique<uvm::UvmUnpairedObjection>();
}
}
