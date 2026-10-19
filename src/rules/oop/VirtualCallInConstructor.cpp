#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::oop {

class VirtualCallVisitor : public slang::ast::ASTVisitor<VirtualCallVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit VirtualCallVisitor(core::DiagnosticEngine& diags, std::string_view className)
        : diags(diags), className(className) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.subroutine.index() == 0) {
            const auto* sub = std::get<0>(call.subroutine);
            if (sub && sub->flags.has(slang::ast::MethodFlags::Virtual)) {
                diags.addViolation(
                    "SV-OOP-005",
                    "VirtualCallInConstructor",
                    "Virtual method '" + std::string(sub->name) + "' is invoked inside constructor 'new' of class '" +
                    std::string(className) + "'. In SystemVerilog, base class constructors execute before derived class members are initialized. "
                    "Calling virtual methods in constructors executes overridden child methods against uninitialized state.",
                    core::Severity::Warning,
                    call.sourceRange.start()
                );
            }
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view className;
};

class VirtualCallInConstructor : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-005"; }
    std::string_view getName() const override { return "VirtualCallInConstructor"; }
    std::string_view getDescription() const override {
        return "Never call virtual methods inside constructors. In SystemVerilog, base constructors execute "
               "before child members are initialized, resulting in child methods operating on uninitialized data.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "new" && !sub.flags.has(slang::ast::MethodFlags::BuiltIn)) {
                        VirtualCallVisitor visitor(diags, cls->name);
                        sub.getBody().visit(visitor);
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualCallInConstructor() {
    return std::make_unique<oop::VirtualCallInConstructor>();
}
}
