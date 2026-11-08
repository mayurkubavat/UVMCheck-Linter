// Prints every top-level operator expression Slang's parser builds in one
// file (with what it includes), one per line, in a canonical form: each
// binary, unary, conditional and inside expression is written in prefix
// form, `(+ a (* b c))`, and everything else as its tokens without trivia.
// "Top-level" means no operator expression above it. Attributes inside
// an expression are left out. The reference for tools/gates/cst_parity.py
// --exprs.
//
//   uvmcheck_cst_exprs [+incdir+DIR]... [+define+NAME[=VALUE]]... FILE
#include <cstdio>
#include <string>

#include "slang/parsing/Preprocessor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"
#include "slang/util/Bag.h"

using namespace slang;
using namespace slang::syntax;
using namespace slang::parsing;

namespace {

bool isOp(SyntaxKind k) {
    return BinaryExpressionSyntax::isKind(k) || PrefixUnaryExpressionSyntax::isKind(k) ||
           PostfixUnaryExpressionSyntax::isKind(k) || ConditionalExpressionSyntax::isKind(k) ||
           InsideExpressionSyntax::isKind(k);
}

// Kinds left out of the comparison while the Rust parser brings them up.
// Empty since slice 4: every construct is compared.
bool skipped(SyntaxKind) {
    return false;
}

void canon(const SyntaxNode* n, std::string& s);

void child(const SyntaxNode* n, size_t i, std::string& s) {
    if (auto t = n->childToken(i)) {
        s += t.rawText();
    }
    else if (auto c = n->childNode(i)) {
        canon(c, s);
    }
}

void all(const SyntaxNode* n, std::string& s) {
    for (size_t i = 0; i < n->getChildCount(); ++i)
        child(n, i, s);
}

// Tokens only, for regions the Rust parser keeps as written.
void plain(const SyntaxNode* n, std::string& s) {
    for (size_t i = 0; i < n->getChildCount(); ++i) {
        if (auto t = n->childToken(i))
            s += t.rawText();
        else if (auto c = n->childNode(i))
            plain(c, s);
    }
}

void canon(const SyntaxNode* n, std::string& s) {
    if (n->kind == SyntaxKind::AttributeInstance) {
        // Attributes are left out of the canonical form.
    }
    else if (skipped(n->kind)) {
        plain(n, s);
    }
    else if (BinaryExpressionSyntax::isKind(n->kind)) {
        auto& b = n->as<BinaryExpressionSyntax>();
        s += "(";
        s += b.operatorToken.rawText();
        s += " ";
        canon(b.left, s);
        s += " ";
        canon(b.right, s);
        s += ")";
    }
    else if (PrefixUnaryExpressionSyntax::isKind(n->kind)) {
        auto& u = n->as<PrefixUnaryExpressionSyntax>();
        s += "(";
        s += u.operatorToken.rawText();
        s += " ";
        canon(u.operand, s);
        s += ")";
    }
    else if (PostfixUnaryExpressionSyntax::isKind(n->kind)) {
        auto& u = n->as<PostfixUnaryExpressionSyntax>();
        s += "(post";
        s += u.operatorToken.rawText();
        s += " ";
        canon(u.operand, s);
        s += ")";
    }
    else if (n->kind == SyntaxKind::ConditionalExpression) {
        auto& c = n->as<ConditionalExpressionSyntax>();
        s += "(? ";
        canon(c.predicate, s);
        s += " ";
        canon(c.left, s);
        s += " ";
        canon(c.right, s);
        s += ")";
    }
    else if (n->kind == SyntaxKind::InsideExpression) {
        auto& c = n->as<InsideExpressionSyntax>();
        s += "(inside ";
        canon(c.expr, s);
        s += " ";
        canon(c.ranges, s);
        s += ")";
    }
    else {
        all(n, s);
    }
}

void walk(const SyntaxNode* n, bool under) {
    if (!n || skipped(n->kind))
        return;
    if (isOp(n->kind) && !under) {
        std::string s;
        canon(n, s);
        std::printf("%s\n", s.c_str());
        under = true;
    }
    for (size_t i = 0; i < n->getChildCount(); ++i)
        if (auto c = n->childNode(i))
            walk(c, under);
}

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
    walk(&tree.value()->root(), false);
    size_t errors = 0;
    for (auto& d : tree.value()->diagnostics())
        if (d.isError()) ++errors;
    std::printf("errors %zu\n", errors);
    return 0;
}
