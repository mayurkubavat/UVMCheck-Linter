#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include <unordered_set>

namespace uvmcheck::rules::uvm {

class TlmConnectVisitor : public slang::ast::ASTVisitor<TlmConnectVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::CallExpression& expr) {
        if (expr.getSubroutineName() == "connect" && expr.syntax) {
            std::string text(expr.syntax->toString());
            connectedCallTexts.push_back(text);
        }
        visitDefault(expr);
    }

    std::vector<std::string> connectedCallTexts;
};

class UvmUnconnectedTlmPort : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-001"; }
    std::string_view getName() const override { return "UvmUnconnectedTlmPort"; }
    std::string_view getDescription() const override {
        return "Mandatory TLM ports and exports declared in uvm_component must be connected "
               "in connect_phase to prevent runtime fatal errors or null-pointer dereferences.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

            // Collect TLM port and export properties
            std::vector<const slang::ast::ClassPropertySymbol*> tlmPorts;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    std::string typeStr(prop.getType().toString());

                    bool isTlm = (typeStr.find("_port") != std::string::npos ||
                                  typeStr.find("_export") != std::string::npos ||
                                  typeStr.find("analysis_port") != std::string::npos ||
                                  typeStr.find("analysis_export") != std::string::npos);

                    if (isTlm) {
                        tlmPorts.push_back(&prop);
                    }
                }
            }

            if (tlmPorts.empty()) continue;

            // Visit all subroutines in the class to collect connect() calls
            TlmConnectVisitor visitor;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (!sub.flags.has(slang::ast::MethodFlags::BuiltIn)) {
                        sub.getBody().visit(visitor);
                    }
                }
            }

            // Check if each port was connected
            for (const auto* port : tlmPorts) {
                std::string portName(port->name);
                bool wasConnected = false;

                for (const auto& callText : visitor.connectedCallTexts) {
                    // Check for portName.connect(...) or this.portName.connect(...)
                    if (callText.find(portName + ".connect") != std::string::npos ||
                        callText.find(portName + " .connect") != std::string::npos) {
                        wasConnected = true;
                        break;
                    }
                }

                if (!wasConnected) {
                    diags.addViolation(
                        "UVM-TLM-001",
                        "UvmUnconnectedTlmPort",
                        "TLM port/export '" + portName + "' in component '" +
                        std::string(cls->name) + "' is declared but never connected via .connect(...). "
                        "Unconnected TLM ports can cause simulation fatal errors upon transaction transfer.",
                        core::Severity::Warning,
                        port->location
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmUnconnectedTlmPort() {
    return std::make_unique<uvm::UvmUnconnectedTlmPort>();
}
}
