#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::concurrency {

class MutexTryGetVisitor : public slang::ast::ASTVisitor<MutexTryGetVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit MutexTryGetVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ExpressionStatement& stmt) {
        const auto* expr = &stmt.expr;
        while (expr->kind == slang::ast::ExpressionKind::Conversion) {
            expr = &expr->as<slang::ast::ConversionExpression>().operand();
        }

        if (expr->kind == slang::ast::ExpressionKind::Call) {
            const auto& call = expr->as<slang::ast::CallExpression>();
            std::string subName = std::string(call.getSubroutineName());
            if (subName == "try_get") {
                bool isMutex = false;
                std::string targetName = "semaphore";

                if (call.thisClass()) {
                    std::string typeStr = call.thisClass()->type->toString();
                    if (typeStr == "semaphore" || typeStr.find("semaphore") != std::string::npos ||
                        typeStr.find("mutex") != std::string::npos || typeStr.find("lock") != std::string::npos) {
                        isMutex = true;
                    }

                    if (call.thisClass()->kind == slang::ast::ExpressionKind::NamedValue) {
                        targetName = std::string(call.thisClass()->as<slang::ast::NamedValueExpression>().symbol.name);
                    } else if (call.thisClass()->kind == slang::ast::ExpressionKind::MemberAccess) {
                        targetName = std::string(call.thisClass()->as<slang::ast::MemberAccessExpression>().member.name);
                    } else if (call.thisClass()->syntax) {
                        targetName = std::string(call.thisClass()->syntax->toString());
                        targetName.erase(0, targetName.find_first_not_of(" \t\n\r"));
                        if (size_t end = targetName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            targetName.erase(end + 1);
                        }
                    }

                    if (targetName.find("sem") != std::string::npos ||
                        targetName.find("mutex") != std::string::npos ||
                        targetName.find("lock") != std::string::npos) {
                        isMutex = true;
                    }
                }

                if (isMutex) {
                    diags.addViolation(
                        "SV-CON-008",
                        "MutexTryGetWithoutCheck",
                        "Non-blocking semaphore acquisition '" + targetName + ".try_get()' called without verifying its return code. "
                        "In SystemVerilog (IEEE 1800 § 15.3), try_get() returns 0 when the requested keys are unavailable. "
                        "Discarding the return value causes the thread to execute critical section code without acquiring the mutual exclusion lock, "
                        "risking catastrophic race conditions and data corruption. Wrap the call in 'if (" + targetName + ".try_get(...))'.",
                        core::Severity::Warning,
                        call.sourceRange.start()
                    );
                }
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class MutexTryGetWithoutCheck : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-008"; }
    std::string_view getName() const override { return "MutexTryGetWithoutCheck"; }
    std::string_view getDescription() const override {
        return "Flags non-blocking semaphore or mutex try_get() calls whose return status is ignored, "
               "which risks threads executing critical sections without holding the lock.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        MutexTryGetVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMutexTryGetWithoutCheck() {
    return std::make_unique<concurrency::MutexTryGetWithoutCheck>();
}
}
