// Prints the tokens Slang's preprocessor produces for one file, one per line
// as "<kind> <text>", with escapes for newlines. The reference that
// crates/sv-preproc is checked against (crates/sv-preproc/examples/ppdiff.rs).
//
//   uvmcheck_pp_dump [+incdir+DIR]... [+define+NAME[=VALUE]]... FILE
#include <cstdio>
#include <string>
#include <vector>

#include "slang/diagnostics/Diagnostics.h"
#include "slang/parsing/Preprocessor.h"
#include "slang/text/SourceManager.h"
#include "slang/util/Bag.h"
#include "slang/util/BumpAllocator.h"

using namespace slang;
using namespace slang::parsing;

int main(int argc, char** argv) {
    SourceManager sm;
    PreprocessorOptions ppo;
    std::string file;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a.rfind("+incdir+", 0) == 0) {
            sm.addUserDirectories(a.substr(8));
            ppo.additionalIncludePaths.emplace_back(a.substr(8));
        } else if (a.rfind("+define+", 0) == 0) {
            ppo.predefines.push_back(a.substr(8));
        } else {
            file = a;
        }
    }
    auto buf = sm.readSource(file, nullptr);
    if (!buf) {
        std::fprintf(stderr, "cannot read %s\n", file.c_str());
        return 2;
    }
    BumpAllocator alloc;
    Diagnostics diags;
    Bag options;
    options.set(ppo);
    Preprocessor pp(sm, alloc, diags, options);
    pp.pushSource(*buf);
    for (;;) {
        Token t = pp.next();
        if (t.kind == TokenKind::EndOfFile) break;
        std::string text(t.rawText());
        std::string out;
        for (char c : text) {
            if (c == '\n') out += "\\n";
            else if (c == '\r') out += "\\r";
            else out += c;
        }
        std::printf("%s\n", out.c_str());
    }
    return 0;
}
