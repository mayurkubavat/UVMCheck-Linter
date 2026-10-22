#include "uvmcheck/uvm/ObjectionControlFlow.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/CallExpression.h"

namespace uvmcheck::uvm {

static bool isCallTo(const slang::ast::Expression& expr, std::string_view targetName) {
    if (expr.kind == slang::ast::ExpressionKind::Call) {
        const auto& call = expr.as<slang::ast::CallExpression>();
        if (call.getSubroutineName() == targetName) {
            return true;
        }
    }
    if (expr.syntax) {
        std::string s(expr.syntax->toString());
        if (s.find(targetName) != std::string::npos) {
            return true;
        }
    }
    return false;
}

std::vector<FlowViolation> ObjectionControlFlowAnalyzer::analyzeSubroutine(
    const slang::ast::SubroutineSymbol& sub,
    std::string_view className) {
    
    std::vector<FlowViolation> violations;
    const auto& body = sub.getBody();

    ObjectionState outState = ObjectionState::Unheld;
    bool returned = evaluateStatement(body, ObjectionState::Unheld, outState, violations, sub, className);

    // If the routine does not unconditionally return, verify state at final exit node
    if (!returned && outState == ObjectionState::Held) {
        violations.push_back({
            "UVM-OBJ-003",
            "ObjectionControlFlowMismatch",
            "Subroutine '" + std::string(sub.name) + "' in class '" + std::string(className) +
            "' reaches routine exit while a phase objection remains held. "
            "Every execution path must call 'phase.drop_objection()' before completion to prevent simulation hangs.",
            core::Severity::Error,
            sub.location
        });
    }

    return violations;
}

bool ObjectionControlFlowAnalyzer::evaluateStatement(
    const slang::ast::Statement& stmt,
    ObjectionState inState,
    ObjectionState& outState,
    std::vector<FlowViolation>& violations,
    const slang::ast::SubroutineSymbol& sub,
    std::string_view className) {
    
    outState = inState;

    switch (stmt.kind) {
        case slang::ast::StatementKind::Block: {
            const auto& block = stmt.as<slang::ast::BlockStatement>();
            return evaluateStatement(block.body, inState, outState, violations, sub, className);
        }

        case slang::ast::StatementKind::List: {
            const auto& list = stmt.as<slang::ast::StatementList>();
            ObjectionState curr = inState;
            for (const auto* s : list.list) {
                if (!s) continue;
                ObjectionState next = curr;
                bool ret = evaluateStatement(*s, curr, next, violations, sub, className);
                if (ret) {
                    outState = next;
                    return true; // Control flow terminated early
                }
                curr = next;
            }
            outState = curr;
            return false;
        }

        case slang::ast::StatementKind::ExpressionStatement: {
            const auto& exprStmt = stmt.as<slang::ast::ExpressionStatement>();
            if (isCallTo(exprStmt.expr, "raise_objection")) {
                if (inState == ObjectionState::Held) {
                    violations.push_back({
                        "UVM-OBJ-003",
                        "ObjectionControlFlowMismatch",
                        "Redundant 'raise_objection()' call in '" + std::string(sub.name) +
                        "': objection is already actively held along this control-flow path.",
                        core::Severity::Warning,
                        exprStmt.sourceRange.start()
                    });
                }
                outState = ObjectionState::Held;
            } else if (isCallTo(exprStmt.expr, "drop_objection")) {
                if (inState == ObjectionState::Unheld) {
                    violations.push_back({
                        "UVM-OBJ-003",
                        "ObjectionControlFlowMismatch",
                        "Premature 'drop_objection()' in '" + std::string(sub.name) +
                        "': no active objection was raised along this execution path. "
                        "Dropping unraised objections causes UVM runtime fatal errors.",
                        core::Severity::Error,
                        exprStmt.sourceRange.start()
                    });
                }
                outState = ObjectionState::Unheld;
            }
            return false;
        }

        case slang::ast::StatementKind::Timed: {
            // '#100 phase.drop_objection(this);' and '@(ev) ...': the delay does not change the state.
            const auto& timed = stmt.as<slang::ast::TimedStatement>();
            return evaluateStatement(timed.stmt, inState, outState, violations, sub, className);
        }

        case slang::ast::StatementKind::Return: {
            if (inState == ObjectionState::Held) {
                violations.push_back({
                    "UVM-OBJ-003",
                    "ObjectionControlFlowMismatch",
                    "Early 'return' in '" + std::string(sub.name) + "' while phase objection is still held. "
                    "Bypassing drop_objection() causes simulation to hang indefinitely.",
                    core::Severity::Error,
                    stmt.sourceRange.start()
                });
            }
            outState = inState;
            return true; // Statement unconditionally exits subroutine
        }

        case slang::ast::StatementKind::Conditional: {
            const auto& cond = stmt.as<slang::ast::ConditionalStatement>();
            ObjectionState trueOut = inState;
            ObjectionState falseOut = inState;

            bool trueRet = evaluateStatement(cond.ifTrue, inState, trueOut, violations, sub, className);
            bool falseRet = false;
            if (cond.ifFalse) {
                falseRet = evaluateStatement(*cond.ifFalse, inState, falseOut, violations, sub, className);
            }

            if (trueRet && falseRet) {
                // Both branches return
                outState = trueOut;
                return true;
            } else if (trueRet) {
                // Only true branch returns, continuation is false branch
                outState = falseOut;
                return false;
            } else if (falseRet) {
                // Only false branch returns, continuation is true branch
                outState = trueOut;
                return false;
            }

            // Both branches continue: check for state divergence
            if (trueOut != falseOut) {
                violations.push_back({
                    "UVM-OBJ-003",
                    "ObjectionControlFlowMismatch",
                    "Asymmetric objection state across conditional branch in '" + std::string(sub.name) +
                    "': 'if' branch leaves objection as " + objectionStateToString(trueOut) +
                    ", while 'else' branch leaves objection as " + objectionStateToString(falseOut) + ". "
                    "Unbalanced branch objections lead to unpredictable simulation termination or runtime fatals.",
                    core::Severity::Error,
                    stmt.sourceRange.start()
                });
                outState = ObjectionState::Unknown;
            } else {
                outState = trueOut;
            }
            return false;
        }

        case slang::ast::StatementKind::Case: {
            const auto& caseStmt = stmt.as<slang::ast::CaseStatement>();
            std::vector<ObjectionState> branchOuts;
            bool allReturn = true;

            for (const auto& item : caseStmt.items) {
                ObjectionState itemOut = inState;
                bool ret = evaluateStatement(*item.stmt, inState, itemOut, violations, sub, className);
                if (!ret) allReturn = false;
                branchOuts.push_back(itemOut);
            }

            if (caseStmt.defaultCase) {
                ObjectionState defOut = inState;
                bool ret = evaluateStatement(*caseStmt.defaultCase, inState, defOut, violations, sub, className);
                if (!ret) allReturn = false;
                branchOuts.push_back(defOut);
            } else {
                // Implicit fallthrough if condition doesn't match any case
                allReturn = false;
                branchOuts.push_back(inState);
            }

            if (allReturn) {
                outState = branchOuts.empty() ? inState : branchOuts.front();
                return true;
            }

            // Check if any branches diverged
            bool diverged = false;
            for (size_t i = 1; i < branchOuts.size(); ++i) {
                if (branchOuts[i] != branchOuts[0]) {
                    diverged = true;
                    break;
                }
            }

            if (diverged) {
                violations.push_back({
                    "UVM-OBJ-003",
                    "ObjectionControlFlowMismatch",
                    "Asymmetric objection state in case statement inside '" + std::string(sub.name) +
                    "'. Different case choices result in divergent objection states.",
                    core::Severity::Error,
                    stmt.sourceRange.start()
                });
                outState = ObjectionState::Unknown;
            } else if (!branchOuts.empty()) {
                outState = branchOuts.front();
            }
            return false;
        }

        default:
            return false;
    }
}

} // namespace uvmcheck::uvm
