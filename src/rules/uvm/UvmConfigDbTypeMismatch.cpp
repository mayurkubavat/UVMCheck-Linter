#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include <unordered_map>

namespace uvmcheck::rules::uvm {

struct ConfigCallInfo {
    std::string typeName;
    slang::SourceLocation location;
};

class ConfigDbVisitor : public slang::ast::ASTVisitor<ConfigDbVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::CallExpression& expr) {
        std::string_view subName = expr.getSubroutineName();
        if (subName == "set" || subName == "get") {
            bool isConfigDb = false;
            if (expr.syntax) {
                std::string text(expr.syntax->toString());
                if (text.find("uvm_config_db") != std::string::npos) {
                    isConfigDb = true;
                }
            }
            if (!isConfigDb && expr.subroutine.index() == 0) {
                const auto* sub = std::get<0>(expr.subroutine);
                if (sub && sub->getParentScope()) {
                    std::string parentName = std::string(sub->getParentScope()->asSymbol().name);
                    if (parentName.find("uvm_config_db") != std::string::npos) {
                        isConfigDb = true;
                    }
                }
            }

            if (isConfigDb) {
                auto args = expr.arguments();
                // uvm_config_db#(T)::set/get(cntxt, inst_name, field_name, value)
                if (args.size() >= 4) {
                    const auto* arg2 = args[2];
                    while (arg2 && arg2->kind == slang::ast::ExpressionKind::Conversion) {
                        arg2 = &arg2->as<slang::ast::ConversionExpression>().operand();
                    }

                    std::string fieldName;
                    if (arg2 && arg2->kind == slang::ast::ExpressionKind::StringLiteral) {
                        fieldName = std::string(arg2->as<slang::ast::StringLiteral>().getValue());
                    } else if (arg2 && arg2->syntax) {
                        fieldName = std::string(arg2->syntax->toString());
                        while (!fieldName.empty() && (fieldName.front() == ' ' || fieldName.front() == '\t')) fieldName.erase(0, 1);
                        while (!fieldName.empty() && (fieldName.back() == ' ' || fieldName.back() == '\t')) fieldName.pop_back();
                        if (fieldName.size() >= 2 && fieldName.front() == '"' && fieldName.back() == '"') {
                            fieldName = fieldName.substr(1, fieldName.size() - 2);
                        }
                    }

                    if (!fieldName.empty()) {
                        std::string valType = std::string(args[3]->type->toString());

                        if (subName == "set") {
                            setCalls[fieldName] = ConfigCallInfo{valType, expr.sourceRange.start()};
                        } else if (subName == "get") {
                            getCalls.push_back({fieldName, ConfigCallInfo{valType, expr.sourceRange.start()}});
                        }
                    }
                }
            }
        }
        visitDefault(expr);
    }

    std::unordered_map<std::string, ConfigCallInfo> setCalls;
    std::vector<std::pair<std::string, ConfigCallInfo>> getCalls;
};

class UvmConfigDbTypeMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-001"; }
    std::string_view getName() const override { return "UvmConfigDbTypeMismatch"; }
    std::string_view getDescription() const override {
        return "The type parameter in uvm_config_db#(T)::get must match the type parameter "
               "used in the corresponding set call for the same field name.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConfigDbVisitor visitor;
        compilation.getRoot().visit(visitor);

        for (const auto& [fieldName, getInfo] : visitor.getCalls) {
            auto it = visitor.setCalls.find(fieldName);
            if (it != visitor.setCalls.end()) {
                const auto& setInfo = it->second;
                if (setInfo.typeName != getInfo.typeName) {
                    diags.addViolation(
                        "UVM-CFG-001",
                        "UvmConfigDbTypeMismatch",
                        "Type mismatch in 'uvm_config_db' for field '" + fieldName +
                        "': value was configured as type '" + setInfo.typeName +
                        "', but is retrieved as type '" + getInfo.typeName +
                        "'. Mismatched types prevent configuration retrieval at runtime.",
                        core::Severity::Error,
                        getInfo.location
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmConfigDbTypeMismatch() {
    return std::make_unique<uvm::UvmConfigDbTypeMismatch>();
}
}
