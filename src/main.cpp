#include "uvmcheck/core/Driver.h"

int main(int argc, char** argv) {
    uvmcheck::core::LinterDriver driver;
    if (!driver.parseCommandLine(argc, argv)) {
        return 1;
    }
    return driver.run();
}
