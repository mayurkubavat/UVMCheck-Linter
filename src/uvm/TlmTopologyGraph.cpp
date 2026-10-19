#include "uvmcheck/uvm/TlmTopologyGraph.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::uvm {

class TlmConnectionCollector : public slang::ast::ASTVisitor<TlmConnectionCollector, slang::ast::VisitFlags::AllGood> {
public:
    explicit TlmConnectionCollector(std::vector<TlmConnectionRecord>& outConns) : connections(outConns) {}

    void handle(const slang::ast::ClassType& cls) {
        std::string prev = currentClass;
        currentClass = std::string(cls.name);
        visitDefault(cls);
        currentClass = prev;
    }

    void handle(const slang::ast::CallExpression& expr) {
        if (expr.getSubroutineName() == "connect" && expr.syntax) {
            std::string text(expr.syntax->toString());
            auto args = expr.arguments();
            std::string arg0Text;
            if (!args.empty() && args[0] && args[0]->syntax) {
                arg0Text = std::string(args[0]->syntax->toString());
            }

            connections.push_back({
                currentClass,
                text,
                arg0Text,
                expr.sourceRange.start()
            });
        }
        visitDefault(expr);
    }

private:
    std::vector<TlmConnectionRecord>& connections;
    std::string currentClass;
};

TlmTopologyGraph::TlmTopologyGraph(slang::ast::Compilation& compilation, const UvmModel& uvmModel) {
    discoverPorts(uvmModel);
    discoverConnections(compilation);
}

void TlmTopologyGraph::discoverPorts(const UvmModel& uvmModel) {
    for (const auto* cls : uvmModel.getAllClasses()) {
        if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

        for (const auto& member : cls->members()) {
            if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                std::string typeStr(prop.getType().toString());

                bool isPull = (typeStr.find("seq_item_pull_port") != std::string::npos);
                bool isAnalysis = (typeStr.find("analysis_port") != std::string::npos);
                bool isStandardPort = (typeStr.find("_port") != std::string::npos);

                if (isPull || isAnalysis || isStandardPort) {
                    declaredPorts.push_back({
                        std::string(cls->name),
                        std::string(prop.name),
                        typeStr,
                        isPull,
                        isAnalysis,
                        prop.location
                    });
                }
            }
        }
    }
}

void TlmTopologyGraph::discoverConnections(slang::ast::Compilation& compilation) {
    TlmConnectionCollector collector(connections);
    compilation.getRoot().visit(collector);
}

std::vector<TlmPortDecl> TlmTopologyGraph::findUnconnectedPorts() const {
    std::vector<TlmPortDecl> unconnected;

    for (const auto& port : declaredPorts) {
        // Focus primarily on mandatory driver pull ports and declared communication ports
        if (!port.isPullPort && !port.isAnalysisPort) continue;

        bool wasConnected = false;
        for (const auto& conn : connections) {
            // Check if connection string references the port name
            if (conn.sourceExpr.find(port.portName) != std::string::npos ||
                conn.targetExpr.find(port.portName) != std::string::npos) {
                wasConnected = true;
                break;
            }
        }

        if (!wasConnected) {
            unconnected.push_back(port);
        }
    }

    return unconnected;
}

} // namespace uvmcheck::uvm
