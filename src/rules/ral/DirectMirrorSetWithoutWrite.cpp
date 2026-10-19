#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"
#include <string>

namespace uvmcheck::rules::ral {

class DirectMirrorVisitor : public slang::ast::ASTVisitor<DirectMirrorVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit DirectMirrorVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.getSubroutineName() == "set") {
            std::string targetName = "reg";
            bool isRalTarget = false;

            if (call.thisClass()) {
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

                if (targetName.find("reg") != std::string::npos ||
                    targetName.find("field") != std::string::npos ||
                    targetName.find("model") != std::string::npos ||
                    targetName.find("ral") != std::string::npos) {
                    isRalTarget = true;
                }
            }

            if (isRalTarget) {
                diags.addViolation(
                    "RAL-ACC-001",
                    "DirectMirrorSetWithoutWrite",
                    "Direct mirror update '" + targetName + ".set(...)' modifies only the local software mirror table without issuing a bus transaction to the physical DUT. In UVM RAL architecture (IEEE 1800.2 § 18.5), invoke 'write()' or follow with 'update()' to program physical hardware and maintain register state synchronization.",
                    core::Severity::Warning,
                    call.sourceRange.start()
                );
            }
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
};

class DirectMirrorSetWithoutWrite : public Rule {
public:
    std::string_view getId() const override { return "RAL-ACC-001"; }
    std::string_view getName() const override { return "DirectMirrorSetWithoutWrite"; }
    std::string_view getDescription() const override {
        return "Flags direct reg.set(...) calls that update only software mirrors without issuing physical DUT write transactions.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        DirectMirrorVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::ral

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDirectMirrorSetWithoutWrite() {
    return std::make_unique<ral::DirectMirrorSetWithoutWrite>();
}
} // namespace uvmcheck::rules
