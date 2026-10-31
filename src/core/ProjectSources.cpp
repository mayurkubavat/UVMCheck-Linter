#include "uvmcheck/core/ProjectSources.h"

#include "slang/driver/Driver.h"

#include <iostream>

namespace uvmcheck::core {

bool applyBuildSettings(slang::driver::Driver& driver, const BuildSettings& build, bool addSources) {
    bool ok = true;
    for (const auto& dir : build.includeDirs) {
        if (auto ec = driver.sourceManager.addUserDirectories(dir)) {
            std::cerr << "[uvmcheck] warning: include dir '" << dir << "': " << ec.message() << "\n";
        }
    }
    for (const auto& def : build.defines) driver.options.defines.push_back(def);
    if (!addSources) return true;
    for (const auto& src : build.sources) driver.sourceLoader.addFiles(src);
    for (const auto& fl : build.filelists) {
        ok &= driver.processCommandFiles(fl, /* makeRelative */ true, /* separateUnit */ false);
    }
    return ok;
}

} // namespace uvmcheck::core
