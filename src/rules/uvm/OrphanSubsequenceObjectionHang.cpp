#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/Statement.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class OrphanSubsequenceObjectionHang : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBJ-004"; }
    std::string_view getName() const override { return "OrphanSubsequenceObjectionHang"; }
    std::string_view getDescription() const override {
        return "Detects background processes launched via fork..join_none inside UVM sequences that raise "
               "objections without a deterministic drop path, leaving orphan objections that hang simulation phases.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        class SequenceObjectionVisitor : public slang::ast::ASTVisitor<SequenceObjectionVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            core::DiagnosticEngine& diags;
            std::string sequenceClassName;

            SequenceObjectionVisitor(core::DiagnosticEngine& d, std::string_view name)
                : diags(d), sequenceClassName(name) {}

            void handle(const slang::ast::BlockStatement& forkStmt) {
                if (forkStmt.blockKind == slang::ast::StatementBlockKind::JoinNone) {
                    std::vector<const slang::ast::Statement*> threadStmts;
                    if (forkStmt.body.kind == slang::ast::StatementKind::List) {
                        for (const auto* stmt : forkStmt.body.as<slang::ast::StatementList>().list) {
                            if (stmt) threadStmts.push_back(stmt);
                        }
                    } else if (forkStmt.body.kind != slang::ast::StatementKind::Empty) {
                        threadStmts.push_back(&forkStmt.body);
                    }

                    for (const auto* stmt : threadStmts) {
                        bool hasRaise = false;
                        bool hasDrop = false;
                        slang::SourceLocation raiseLoc;

                        class ThreadObjectionScanner : public slang::ast::ASTVisitor<ThreadObjectionScanner, slang::ast::VisitFlags::AllGood> {
                        public:
                            bool& raise;
                            bool& drop;
                            slang::SourceLocation& loc;
                            ThreadObjectionScanner(bool& r, bool& d, slang::SourceLocation& l)
                                : raise(r), drop(d), loc(l) {}

                            void handle(const slang::ast::CallExpression& call) {
                                std::string name(call.getSubroutineName());
                                if (name == "raise_objection") {
                                    raise = true;
                                    loc = call.sourceRange.start();
                                } else if (name == "drop_objection") {
                                    drop = true;
                                }
                                visitDefault(call);
                            }
                        };

                        ThreadObjectionScanner scanner(hasRaise, hasDrop, raiseLoc);
                        stmt->visit(scanner);

                        if (hasRaise && !hasDrop) {
                            diags.addViolation(
                                "UVM-OBJ-004",
                                "OrphanSubsequenceObjectionHang",
                                "Asynchronous background process spawned via fork..join_none in sequence '" +
                                sequenceClassName + "' raises an objection without a local drop_objection() call. "
                                "When the parent sequence finishes, this orphan objection will remain active, causing a phase timeout simulation hang.",
                                core::Severity::Error,
                                raiseLoc
                            );
                        }
                    }
                }
                visitDefault(forkStmt);
            }
        };

        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmSequence(*cls)) continue;

            SequenceObjectionVisitor visitor(diags, cls->name);
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    member.as<slang::ast::SubroutineSymbol>().getBody().visit(visitor);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOrphanSubsequenceObjectionHang() {
    return std::make_unique<uvm::OrphanSubsequenceObjectionHang>();
}
}
