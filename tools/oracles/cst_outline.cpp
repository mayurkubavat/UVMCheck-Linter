// Prints the outline Slang's parser finds in one file (with what it
// includes): one line per declaration, "<kind> <name>", in source order,
// then "errors <n>" for the parse diagnostics. The reference that
// crates/sv-syntax's outline is checked against (tools/gates/cst_parity.py).
//
//   uvmcheck_cst_outline [+incdir+DIR]... [+define+NAME[=VALUE]]... FILE
#include <cstdio>
#include <cstdlib>
#include <string>

#include "slang/diagnostics/DiagnosticEngine.h"
#include "slang/parsing/Preprocessor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"
#include "slang/util/Bag.h"

using namespace slang;
using namespace slang::syntax;
using namespace slang::parsing;

namespace {

// Token text of a node without trivia.
std::string bare(const SyntaxNode* n) {
    std::string s;
    if (!n) return s;
    for (size_t i = 0; i < n->getChildCount(); ++i) {
        if (auto t = n->childToken(i)) s += t.rawText();
        else if (auto c = n->childNode(i)) s += bare(c);
    }
    return s;
}

struct Outline : SyntaxVisitor<Outline> {
    void line(const char* kind, const std::string& name) { std::printf("%s %s\n", kind, name.c_str()); }

    void handle(const ModuleDeclarationSyntax& n) {
        const char* k = n.kind == SyntaxKind::PackageDeclaration     ? "package"
                        : n.kind == SyntaxKind::InterfaceDeclaration ? "interface"
                        : n.kind == SyntaxKind::ProgramDeclaration   ? "program"
                                                                     : "module";
        line(k, std::string(n.header->name.rawText()));
        visitDefault(n);
    }
    void handle(const ClassDeclarationSyntax& n) {
        line("class", std::string(n.name.rawText()));
        visitDefault(n);
    }
    void handle(const FunctionDeclarationSyntax& n) {
        line(n.kind == SyntaxKind::TaskDeclaration ? "task" : "function", bare(n.prototype->name));
        visitDefault(n);
    }
    void handle(const ClassMethodPrototypeSyntax& n) {
        line(n.prototype->keyword.kind == TokenKind::TaskKeyword ? "task" : "function", bare(n.prototype->name));
    }
    void handle(const TypedefDeclarationSyntax& n) {
        line("typedef", std::string(n.name.rawText()));
        visitDefault(n);
    }
    void handle(const ConstraintDeclarationSyntax& n) {
        line("constraint", bare(n.name));
    }
    void handle(const ConstraintPrototypeSyntax& n) {
        line("constraint", bare(n.name));
    }
    void handle(const CovergroupDeclarationSyntax& n) {
        line("covergroup", std::string(n.name.rawText()));
    }
};

} // namespace

int main(int argc, char** argv) {
    auto sm = std::make_shared<SourceManager>();
    PreprocessorOptions ppo;
    std::string file;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a.rfind("+incdir+", 0) == 0) {
            sm->addUserDirectories(a.substr(8));
            ppo.additionalIncludePaths.emplace_back(a.substr(8));
        } else if (a.rfind("+define+", 0) == 0) {
            ppo.predefines.push_back(a.substr(8));
        } else {
            file = a;
        }
    }
    Bag options;
    options.set(ppo);
    auto tree = SyntaxTree::fromFile(file, *sm, options);
    if (!tree) {
        std::fprintf(stderr, "cannot read %s\n", file.c_str());
        return 2;
    }
    Outline o;
    tree.value()->root().visit(o);
    size_t errors = 0;
    for (auto& d : tree.value()->diagnostics())
        if (d.isError()) {
            ++errors;
            if (std::getenv("CST_OUTLINE_DIAGS"))
                std::fprintf(stderr, "%s:%zu: %s\n", std::string(sm->getFileName(d.location)).c_str(),
                             sm->getLineNumber(d.location), std::string(toString(d.code)).c_str());
        }
    std::printf("errors %zu\n", errors);
    return 0;
}
