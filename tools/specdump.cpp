// How often uvmcheck's shared AST walk visits the body of each generic class:
// once per specialization, after the engine's DefaultSpecializer (copied
// from src/engine/RuleEngine.cpp) has made every default specialization.
// Prints `<count> <name>@<file>:<line>`. The reference for tools/spec_gate.py.
//
//   B=build/_deps; c++ -std=c++20 -O1 -I$B/slang-src/include -I$B/slang-build/source \
//     -I$B/fmt-src/include -I$B/slang-src/external tools/specdump.cpp \
//     $B/slang-build/lib/libsvlang.a $B/slang-build/lib/libmimalloc.a -lpthread -o build/specdump
#include <slang/ast/ASTVisitor.h>
#include <slang/ast/Compilation.h>
#include <slang/ast/types/AllTypes.h>
#include <slang/driver/Driver.h>
#include <filesystem>
#include <cstdlib>
#include <map>
#include <iostream>
using namespace slang; using namespace slang::ast;
#include <unordered_set>
#include <vector>
class DefaultSpecializer
    : public slang::ast::ASTVisitor<DefaultSpecializer, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::GenericClassDefSymbol& gen) {
        if (knownGenerics.insert(&gen).second) generics.push_back(&gen);
        (void)gen.getDefaultSpecialization(*gen.getParentScope());
        visitSpecializations(gen);
    }

    void finish() {
        size_t before;
        do {
            before = seen.size();
            // Visiting can discover more generic classes; index, don't iterate.
            for (size_t i = 0; i < generics.size(); i++) visitSpecializations(*generics[i]);
        } while (seen.size() != before);
    }

private:
    void visitSpecializations(const slang::ast::GenericClassDefSymbol& gen) {
        std::vector<const slang::ast::Type*> toVisit;
        for (const auto& spec : gen.specializations()) {
            if (seen.insert(&spec).second) toVisit.push_back(&spec);
        }
        for (const auto* spec : toVisit) spec->visit(*this);
    }

    std::vector<const slang::ast::GenericClassDefSymbol*> generics;
    std::unordered_set<const slang::ast::GenericClassDefSymbol*> knownGenerics;
    std::unordered_set<const slang::ast::Type*> seen;
};


struct V : ASTVisitor<V, VisitFlags::AllGood> {
  std::map<std::string,int> n; const SourceManager* sm;
  void handle(const ClassType& t) {
    if (t.genericClass) {
      if (const char* only = std::getenv("SPECDUMP_LIST"); only && t.genericClass->name == only)
        std::cerr << t.toString() << "\n";
      auto loc = t.genericClass->location;
      n[std::string(t.genericClass->name) + "@" + std::string(std::filesystem::path(sm->getFileName(loc)).filename()) + ":" + std::to_string(sm->getLineNumber(loc))]++;
    }
    visitDefault(t);
  }
};
int main(int argc, char** argv) {
  driver::Driver d; d.addStandardArgs();
  if (!d.parseCommandLine(argc, argv) || !d.processOptions() || !d.parseAllSources()) return 1;
  auto comp = d.createCompilation();
  comp->getAllDiagnostics();
  DefaultSpecializer sp; comp->getRoot().visit(sp); sp.finish();
  V v; v.sm = comp->getSourceManager();
  comp->getRoot().visit(v);
  for (auto& [k, c] : v.n) std::cout << c << " " << k << "\n";
}
