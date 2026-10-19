#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Statement.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"
#include <vector>
#include <string>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::concurrency {

struct LockAcquisition {
    std::string lockName;
    slang::SourceLocation location;
};

class CircularThreadWaitDeadlock : public Rule {
public:
    std::string_view getId() const override { return "SV-CNC-002"; }
    std::string_view getName() const override { return "CircularThreadWaitDeadlock"; }
    std::string_view getDescription() const override {
        return "Detects circular lock acquisition orders and wait-for dependency cycles across concurrent "
               "threads in fork..join blocks, which cause guaranteed or probabilistic simulation deadlocks.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {

        class ForkBlockVisitor : public slang::ast::ASTVisitor<ForkBlockVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            core::DiagnosticEngine& diags;
            explicit ForkBlockVisitor(core::DiagnosticEngine& d) : diags(d) {}

            void handle(const slang::ast::BlockStatement& forkStmt) {
                if (forkStmt.blockKind != slang::ast::StatementBlockKind::JoinAll &&
                    forkStmt.blockKind != slang::ast::StatementBlockKind::JoinAny &&
                    forkStmt.blockKind != slang::ast::StatementBlockKind::JoinNone) {
                    visitDefault(forkStmt);
                    return;
                }

                // Each parallel statement or block in the fork represents a thread
                std::vector<std::vector<LockAcquisition>> threadLockSequences;

                class ThreadLockVisitor : public slang::ast::ASTVisitor<ThreadLockVisitor, slang::ast::VisitFlags::AllGood> {
                public:
                    std::vector<LockAcquisition>& locks;
                    explicit ThreadLockVisitor(std::vector<LockAcquisition>& l) : locks(l) {}

                    void handle(const slang::ast::CallExpression& call) {
                        std::string subName(call.getSubroutineName());
                        if (subName == "get") {
                            std::string semName;
                            if (call.thisClass() && call.thisClass()->syntax) {
                                semName = call.thisClass()->syntax->toString();
                            } else if (call.syntax) {
                                std::string text(call.syntax->toString());
                                size_t dot = text.find(".get");
                                if (dot != std::string::npos) {
                                    size_t start = text.rfind(' ', dot);
                                    if (start == std::string::npos) start = 0;
                                    else start++;
                                    semName = text.substr(start, dot - start);
                                }
                            }
                            size_t first = semName.find_first_not_of(" \t\n\r");
                            size_t last = semName.find_last_not_of(" \t\n\r");
                            if (first != std::string::npos) {
                                locks.push_back({semName.substr(first, last - first + 1), call.sourceRange.start()});
                            }
                        }
                        visitDefault(call);
                    }
                };

                // Extract thread statements
                std::vector<const slang::ast::Statement*> threadStmts;
                if (forkStmt.body.kind == slang::ast::StatementKind::List) {
                    for (const auto* stmt : forkStmt.body.as<slang::ast::StatementList>().list) {
                        if (stmt) threadStmts.push_back(stmt);
                    }
                } else if (forkStmt.body.kind != slang::ast::StatementKind::Empty) {
                    threadStmts.push_back(&forkStmt.body);
                }

                // Visit each thread branch in the fork block
                for (const auto* stmt : threadStmts) {
                    std::vector<LockAcquisition> threadLocks;
                    ThreadLockVisitor threadVisitor(threadLocks);
                    stmt->visit(threadVisitor);
                    if (threadLocks.size() >= 2) {
                        threadLockSequences.push_back(threadLocks);
                    }
                }

                // Multi-pass cycle detection: compare lock orders across concurrent thread pairs
                for (size_t t1 = 0; t1 < threadLockSequences.size(); ++t1) {
                    for (size_t t2 = t1 + 1; t2 < threadLockSequences.size(); ++t2) {
                        const auto& seq1 = threadLockSequences[t1];
                        const auto& seq2 = threadLockSequences[t2];

                        for (size_t i = 0; i + 1 < seq1.size(); ++i) {
                            const std::string& a = seq1[i].lockName;
                            const std::string& b = seq1[i + 1].lockName;
                            if (a == b) continue;

                            for (size_t j = 0; j + 1 < seq2.size(); ++j) {
                                const std::string& c = seq2[j].lockName;
                                const std::string& d = seq2[j + 1].lockName;

                                // Cycle condition: Thread 1 acquires A then B, while Thread 2 acquires B then A
                                if (a == d && b == c) {
                                    diags.addViolation(
                                        "SV-CNC-002",
                                        "CircularThreadWaitDeadlock",
                                        "Circular lock acquisition order detected between concurrent fork threads: "
                                        "Thread 1 acquires '" + a + "' before '" + b + "', while Thread 2 acquires '" +
                                        b + "' before '" + a + "'. "
                                        "This creates a circular wait dependency and guaranteed simulation deadlock. Enforce a global lock acquisition hierarchy.",
                                        core::Severity::Error,
                                        seq2[j + 1].location
                                    );
                                }
                            }
                        }
                    }
                }

                visitDefault(forkStmt);
            }
        };

        ForkBlockVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCircularThreadWaitDeadlock() {
    return std::make_unique<concurrency::CircularThreadWaitDeadlock>();
}
}
