// The static type slang gives each expression, as uvmcheck's rules read it
// (`Expression::type`): one line per expression the AST walk reaches,
//   <file>:<line>:<col> <expression kind> <bits> <s|u> <4|2> <type>
// with the type as `Type::toString()` prints it. Implicit conversions are not
// printed (rules strip them); their operands are. Lines are unique and sorted,
// so a body walked once per instance or specialization prints each distinct
// type once. The reference for tools/gates/expr_gate.py.
// Built as build/uvmcheck_exprdump.
#include <slang/ast/ASTVisitor.h>
#include <slang/ast/Compilation.h>
#include <slang/ast/expressions/ConversionExpression.h>
#include <slang/ast/types/AllTypes.h>
#include <slang/driver/Driver.h>
#include <filesystem>
#include <iostream>
#include <set>
#include <string>
using namespace slang;
using namespace slang::ast;

struct V : ASTVisitor<V, VisitFlags::AllGood> {
    std::set<std::string> out;
    const SourceManager* sm = nullptr;

    template<typename T>
    void handle(const T& node) {
        if constexpr (std::is_base_of_v<Expression, T>) record(node);
        visitDefault(node);
    }

    void record(const Expression& e) {
        if (e.kind == ExpressionKind::Conversion && e.as<ConversionExpression>().isImplicit()) return;
        if (!e.type || !e.syntax) return;
        auto loc = e.sourceRange.start();
        if (!loc.valid() || sm->isMacroLoc(loc)) return;
        const Type& t = *e.type;
        std::string line = std::filesystem::path(sm->getFileName(loc)).filename().string() + ":" +
                           std::to_string(sm->getLineNumber(loc)) + ":" + std::to_string(sm->getColumnNumber(loc)) +
                           " " + std::string(toString(e.kind)) + " " +
                           std::to_string(t.isIntegral() ? t.getBitWidth() : 0) + " " +
                           (t.isSigned() ? "s" : "u") + " " + (t.isFourState() ? "4" : "2") + " " + t.toString();
        out.insert(line);
    }
};

int main(int argc, char** argv) {
    driver::Driver d;
    d.addStandardArgs();
    if (!d.parseCommandLine(argc, argv) || !d.processOptions() || !d.parseAllSources()) return 1;
    auto comp = d.createCompilation();
    comp->getAllDiagnostics();
    V v;
    v.sm = comp->getSourceManager();
    comp->getRoot().visit(v);
    for (const auto& l : v.out) std::cout << l << "\n";
}
