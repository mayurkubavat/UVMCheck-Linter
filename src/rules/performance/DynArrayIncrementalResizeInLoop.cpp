#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/statements/LoopStatements.h"

namespace uvmcheck::rules::performance {

class DynArrayResizeVisitor : public slang::ast::ASTVisitor<DynArrayResizeVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit DynArrayResizeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ForLoopStatement& loop) {
        loopDepth++;
        visitDefault(loop);
        loopDepth--;
    }

    void handle(const slang::ast::ForeachLoopStatement& loop) {
        loopDepth++;
        visitDefault(loop);
        loopDepth--;
    }

    void handle(const slang::ast::WhileLoopStatement& loop) {
        loopDepth++;
        visitDefault(loop);
        loopDepth--;
    }

    void handle(const slang::ast::DoWhileLoopStatement& loop) {
        loopDepth++;
        visitDefault(loop);
        loopDepth--;
    }

    void handle(const slang::ast::NewArrayExpression& expr) {
        if (loopDepth > 0 && expr.initExpr()) {
            diags.addViolation(
                "SV-PRF-003",
                "DynArrayIncrementalResizeInLoop",
                "Dynamic array resize 'new[...](existing_array)' invoked inside a loop. "
                "Incrementally resizing dynamic arrays creates an O(N^2) heap reallocation and element-copying bottleneck. "
                "Pre-allocate the full array with 'new[size]' before the loop, or use a SystemVerilog queue ([$]).",
                core::Severity::Warning,
                expr.sourceRange.start()
            );
        }
        visitDefault(expr);
    }

private:
    int loopDepth = 0;
    core::DiagnosticEngine& diags;
};

class DynArrayIncrementalResizeInLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-003"; }
    std::string_view getName() const override { return "DynArrayIncrementalResizeInLoop"; }
    std::string_view getDescription() const override {
        return "Growing dynamic arrays incrementally inside loops with new[arr.size()+1](arr) causes O(N^2) memory "
               "reallocation bottlenecks. Pre-allocate with new[N] or use SystemVerilog queues instead.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        DynArrayResizeVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDynArrayIncrementalResizeInLoop() {
    return std::make_unique<performance::DynArrayIncrementalResizeInLoop>();
}
}
