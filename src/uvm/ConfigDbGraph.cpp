#include "uvmcheck/uvm/ConfigDbGraph.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::uvm {

static std::string extractStringArg(const slang::ast::Expression* expr) {
    if (!expr) return "";
    while (expr->kind == slang::ast::ExpressionKind::Conversion) {
        expr = &expr->as<slang::ast::ConversionExpression>().operand();
    }
    if (expr->kind == slang::ast::ExpressionKind::StringLiteral) {
        return std::string(expr->as<slang::ast::StringLiteral>().getValue());
    }
    if (expr->syntax) {
        std::string s(expr->syntax->toString());
        while (!s.empty() && (s.front() == ' ' || s.front() == '\t')) s.erase(0, 1);
        while (!s.empty() && (s.back() == ' ' || s.back() == '\t')) s.pop_back();
        if (s.size() >= 2 && s.front() == '"' && s.back() == '"') {
            return s.substr(1, s.size() - 2);
        }
        return s;
    }
    return "";
}

class ConfigDbCollector : public slang::ast::ASTVisitor<ConfigDbCollector, slang::ast::VisitFlags::AllGood> {
public:
    explicit ConfigDbCollector(std::vector<ConfigDbNode>& nodes) : outNodes(nodes) {}

    void handle(const slang::ast::ClassType& cls) {
        std::string prevClass = currentClass;
        currentClass = std::string(cls.name);
        visitDefault(cls);
        currentClass = prevClass;
    }

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
                if (args.size() >= 4) {
                    std::string instStr = extractStringArg(args[1]);
                    std::string fieldStr = extractStringArg(args[2]);

                    std::string valType;
                    if (args[3] && args[3]->type) {
                        valType = std::string(args[3]->type->toString());
                    }

                    if (!fieldStr.empty() && !valType.empty()) {
                        outNodes.push_back({
                            (subName == "set" ? ConfigDbNode::Kind::Set : ConfigDbNode::Kind::Get),
                            valType,
                            currentClass,
                            instStr,
                            fieldStr,
                            expr.sourceRange.start()
                        });
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    std::vector<ConfigDbNode>& outNodes;
    std::string currentClass;
};

ConfigDbGraph::ConfigDbGraph(slang::ast::Compilation& compilation) {
    collectNodes(compilation);
}

void ConfigDbGraph::collectNodes(slang::ast::Compilation& compilation) {
    ConfigDbCollector collector(nodes);
    compilation.getRoot().visit(collector);
}

std::vector<ConfigDbMismatch> ConfigDbGraph::findMismatches() const {
    std::vector<ConfigDbMismatch> mismatches;

    for (const auto& getNode : nodes) {
        if (getNode.kind != ConfigDbNode::Kind::Get) continue;

        for (const auto& setNode : nodes) {
            if (setNode.kind != ConfigDbNode::Kind::Set) continue;

            if (setNode.fieldName == getNode.fieldName) {
                // If the types differ across components, report mismatch
                if (setNode.typeName != getNode.typeName) {
                    mismatches.push_back({
                        getNode.fieldName,
                        setNode.contextClass,
                        setNode.typeName,
                        getNode.contextClass,
                        getNode.typeName,
                        getNode.location,
                        setNode.location
                    });
                }
            }
        }
    }

    return mismatches;
}

} // namespace uvmcheck::uvm
