#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class UvmDeprecatedApisVisitor : public slang::ast::ASTVisitor<UvmDeprecatedApisVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit UvmDeprecatedApisVisitor(core::DiagnosticEngine& diags, std::string_view targetRule = "")
        : diags(diags), targetRule(targetRule) {}

    bool shouldCheck(std::string_view id) const {
        return targetRule.empty() || targetRule == id;
    }

    void handle(const slang::ast::CallExpression& call) {
        std::string_view subName = call.getSubroutineName();

        // 1. UVM-DEP-001: set_config_* / get_config_*
        if (shouldCheck("UVM-DEP-001") &&
            (subName == "set_config_int" || subName == "set_config_string" || subName == "set_config_object" ||
             subName == "get_config_int" || subName == "get_config_string" || subName == "get_config_object")) {
            diags.addViolation(
                "UVM-DEP-001",
                "UvmDeprecatedConfigCalls",
                "Legacy configuration method '" + std::string(subName) +
                "' is formally deprecated in IEEE 1800.2 (Annex H). "
                "These string-based methods lack type safety and break under +define+UVM_NO_DEPRECATED. "
                "Replace with modern, type-safe 'uvm_config_db#(T)::set(this, ...)' and 'uvm_config_db#(T)::get(this, ...)'.",
                core::Severity::Error,
                call.sourceRange.start()
            );
        }
        // 2. UVM-DEP-002: uvm_report_server::get_server()
        else if (shouldCheck("UVM-DEP-002") && subName == "get_server") {
            if (call.syntax) {
                std::string callText = call.syntax->toString();
                if (callText.find("uvm_report_server") != std::string::npos) {
                    diags.addViolation(
                        "UVM-DEP-002",
                        "UvmDeprecatedReportServer",
                        "Direct access via 'uvm_report_server::get_server()' is deprecated in IEEE 1800.2 § F.4.2 and Annex H. "
                        "UVM report servers are now context-managed through uvm_coreservice_t. "
                        "Replace with: 'uvm_coreservice_t cs = uvm_coreservice_t::get(); server = cs.get_report_server();'.",
                        core::Severity::Warning,
                        call.sourceRange.start()
                    );
                }
            }
        }
        // 3. UVM-DEP-004: uvm_test_done.raise_objection / drop_objection
        else if (shouldCheck("UVM-DEP-004") && (subName == "raise_objection" || subName == "drop_objection")) {
            if (call.syntax) {
                std::string callText = call.syntax->toString();
                if (callText.find("uvm_test_done") != std::string::npos) {
                    diags.addViolation(
                        "UVM-DEP-004",
                        "UvmDeprecatedTestDone",
                        "Legacy OVM objection handle 'uvm_test_done' is obsolete in IEEE 1800.2 Annex H. "
                        "Objections must be attached directly to the active phase handle ('phase." +
                        std::string(subName) + "(this)').",
                        core::Severity::Error,
                        call.sourceRange.start()
                    );
                }
            }
        }

        visitDefault(call);
    }

    // 4. UVM-DEP-003: printer.knobs
    void handle(const slang::ast::MemberAccessExpression& memberAccess) {
        if (shouldCheck("UVM-DEP-003") && memberAccess.member.name == "knobs") {
            std::string receiverType = memberAccess.value().type->toString();
            if (receiverType.find("printer") != std::string::npos || receiverType.find("uvm_printer") != std::string::npos) {
                diags.addViolation(
                    "UVM-DEP-003",
                    "UvmDeprecatedPrinterKnobs",
                    "Direct access to internal 'printer.knobs' is deprecated in IEEE 1800.2 § 5.5.2. "
                    "Use official policy accessor methods on uvm_printer (e.g. 'printer.set_max_depth()', 'printer.set_radix()').",
                    core::Severity::Warning,
                    memberAccess.sourceRange.start()
                );
            }
        }
        visitDefault(memberAccess);
    }

private:
    core::DiagnosticEngine& diags;
    std::string targetRule;
};

class UvmDeprecatedConfigCalls : public Rule {
public:
    std::string_view getId() const override { return "UVM-DEP-001"; }
    std::string_view getName() const override { return "UvmDeprecatedConfigCalls"; }
    std::string_view getDescription() const override {
        return "Disallow deprecated legacy set_config_* and get_config_* methods (IEEE 1800.2 Annex H). "
               "Use type-safe uvm_config_db#(T)::set/get.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmDeprecatedApisVisitor visitor(diags, getId());
        compilation.getRoot().visit(visitor);
    }
};

class UvmDeprecatedReportServer : public Rule {
public:
    std::string_view getId() const override { return "UVM-DEP-002"; }
    std::string_view getName() const override { return "UvmDeprecatedReportServer"; }
    std::string_view getDescription() const override {
        return "Disallow direct singleton calls to uvm_report_server::get_server() (IEEE 1800.2 Annex H). "
               "Obtain report servers through uvm_coreservice_t.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmDeprecatedApisVisitor visitor(diags, getId());
        compilation.getRoot().visit(visitor);
    }
};

class UvmDeprecatedPrinterKnobs : public Rule {
public:
    std::string_view getId() const override { return "UVM-DEP-003"; }
    std::string_view getName() const override { return "UvmDeprecatedPrinterKnobs"; }
    std::string_view getDescription() const override {
        return "Disallow direct access to 'printer.knobs' (IEEE 1800.2 § 5.5.2). "
               "Use accessor methods on uvm_printer.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmDeprecatedApisVisitor visitor(diags, getId());
        compilation.getRoot().visit(visitor);
    }
};

class UvmDeprecatedTestDone : public Rule {
public:
    std::string_view getId() const override { return "UVM-DEP-004"; }
    std::string_view getName() const override { return "UvmDeprecatedTestDone"; }
    std::string_view getDescription() const override {
        return "Disallow obsolete OVM 'uvm_test_done' objection handle (IEEE 1800.2 Annex H). "
               "Attach objections to the active phase parameter.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UvmDeprecatedApisVisitor visitor(diags, getId());
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmDeprecatedConfigCalls() {
    return std::make_unique<deprecated::UvmDeprecatedConfigCalls>();
}
std::unique_ptr<Rule> createUvmDeprecatedReportServer() {
    return std::make_unique<deprecated::UvmDeprecatedReportServer>();
}
std::unique_ptr<Rule> createUvmDeprecatedPrinterKnobs() {
    return std::make_unique<deprecated::UvmDeprecatedPrinterKnobs>();
}
std::unique_ptr<Rule> createUvmDeprecatedTestDone() {
    return std::make_unique<deprecated::UvmDeprecatedTestDone>();
}
}
