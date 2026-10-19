#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class TimingCheckerVisitor : public slang::syntax::SyntaxVisitor<TimingCheckerVisitor> {
public:
    bool hasTiming = false;

    void handle(const slang::syntax::TimingControlStatementSyntax&) {
        hasTiming = true;
    }

    void handle(const slang::syntax::WaitStatementSyntax&) {
        hasTiming = true;
    }

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
            hasTiming = true;
        }
        visitDefault(block);
    }
};

class RepeatLoopVisitor : public slang::syntax::SyntaxVisitor<RepeatLoopVisitor> {
public:
    explicit RepeatLoopVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassMethodDeclarationSyntax& method) {
        bool isTask = (method.declaration->kind == slang::syntax::SyntaxKind::TaskDeclaration ||
                       method.declaration->prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword);

        if (isTask) {
            std::string prevTask = currentTaskName;
            bool prevInside = insideTask;

            currentTaskName = std::string(method.declaration->prototype->name->getLastToken().valueText());
            insideTask = true;

            method.declaration->visit(*this);

            insideTask = prevInside;
            currentTaskName = prevTask;
        } else {
            visitDefault(method);
        }
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        bool isTask = (func.kind == slang::syntax::SyntaxKind::TaskDeclaration ||
                       func.prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword);

        if (isTask) {
            std::string prevTask = currentTaskName;
            bool prevInside = insideTask;

            currentTaskName = std::string(func.prototype->name->getLastToken().valueText());
            insideTask = true;

            visitDefault(func);

            insideTask = prevInside;
            currentTaskName = prevTask;
        } else {
            visitDefault(func);
        }
    }

    void handle(const slang::syntax::LoopStatementSyntax& loop) {
        if (insideTask && loop.repeatOrWhile.kind == slang::parsing::TokenKind::RepeatKeyword) {
            TimingCheckerVisitor timingCheck;
            loop.statement->visit(timingCheck);

            if (!timingCheck.hasTiming) {
                std::string bodyStr = std::string(loop.statement->toString());
                if (bodyStr.find('@') == std::string::npos &&
                    bodyStr.find('#') == std::string::npos &&
                    bodyStr.find("wait") == std::string::npos) {
                    diags.addViolation(
                        "SV-DLY-003",
                        "RepeatLoopWithoutTiming",
                        "Repeat loop inside task '" + currentTaskName +
                        "()' contains no time-consuming statements (#, @, wait). "
                        "In verification testbenches, tasks operate across simulation time; "
                        "a repeat loop without clock or delay synchronization spins instantaneously in a single delta cycle and frequently indicates a missing '@(posedge clk)' or clocking block synchronization.",
                        core::Severity::Warning,
                        loop.repeatOrWhile.location()
                    );
                }
            }
        }
        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
    bool insideTask = false;
    std::string currentTaskName;
};

class RepeatLoopWithoutTiming : public Rule {
public:
    std::string_view getId() const override { return "SV-DLY-003"; }
    std::string_view getName() const override { return "RepeatLoopWithoutTiming"; }
    std::string_view getDescription() const override {
        return "Flags repeat loops inside testbench tasks that contain no time-consuming statements (#, @, wait). "
               "Zero-delay repeat loops in tasks spin instantaneously in a single delta cycle, often indicating missing clock synchronization.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        RepeatLoopVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRepeatLoopWithoutTiming() {
    return std::make_unique<concurrency::RepeatLoopWithoutTiming>();
}
}
