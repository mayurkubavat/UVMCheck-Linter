#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class WaitForkVisitor : public slang::syntax::SyntaxVisitor<WaitForkVisitor> {
public:
    explicit WaitForkVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (!func.prototype || !func.prototype->name) {
            visitDefault(func);
            return;
        }

        std::string routineName = std::string(func.prototype->name->getLastToken().valueText());
        if (routineName.empty()) {
            routineName = std::string(func.prototype->name->toString());
            routineName.erase(0, routineName.find_first_not_of(" \t\n\r"));
            if (size_t end = routineName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                routineName.erase(end + 1);
            }
        }

        std::string prevRoutine = currentRoutine;
        bool prevHasFork = currentHasFork;
        std::vector<slang::SourceLocation> prevWaitForks = std::move(currentWaitForks);

        currentRoutine = routineName;
        currentHasFork = false;
        currentWaitForks.clear();

        visitDefault(func);

        if (!currentHasFork && !currentWaitForks.empty()) {
            for (auto loc : currentWaitForks) {
                diags.addViolation(
                    "SV-CON-009",
                    "WaitForkWithoutFork",
                    "Routine '" + currentRoutine + "' executes 'wait fork;' but contains no parallel 'fork' blocks. "
                    "In SystemVerilog (IEEE 1800 § 9.6.1), 'wait fork' blocks execution until all immediate child processes "
                    "spawned by the current process terminate. Calling 'wait fork' in a routine that spawned zero child threads "
                    "either executes as a useless no-op or causes unintended hangs by waiting on unrelated threads spawned by caller processes.",
                    core::Severity::Warning,
                    loc
                );
            }
        }

        currentRoutine = prevRoutine;
        currentHasFork = prevHasFork;
        currentWaitForks = std::move(prevWaitForks);
    }

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (!currentRoutine.empty() && block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
            currentHasFork = true;
        }
        visitDefault(block);
    }

    void handle(const slang::syntax::WaitForkStatementSyntax& stmt) {
        if (!currentRoutine.empty()) {
            currentWaitForks.push_back(stmt.sourceRange().start());
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
    std::string currentRoutine;
    bool currentHasFork = false;
    std::vector<slang::SourceLocation> currentWaitForks;
};

class WaitForkWithoutFork : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-009"; }
    std::string_view getName() const override { return "WaitForkWithoutFork"; }
    std::string_view getDescription() const override {
        return "Flags 'wait fork;' statements in tasks or routines that contain no 'fork' blocks. "
               "Calling wait fork without spawning child processes can cause unintended hangs or dead code.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        WaitForkVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createWaitForkWithoutFork() {
    return std::make_unique<concurrency::WaitForkWithoutFork>();
}
}
