cmake_minimum_required(VERSION 3.20)
project(HUSH CXX)

if(NOT APPLE)
    message(STATUS "HUSH is a macOS AudioServerPlugIn; nothing to build on this host")
    return()
endif()

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if(NOT CMAKE_OSX_DEPLOYMENT_TARGET)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "12.0" CACHE STRING "Minimum macOS version" FORCE)
endif()

set(CODESIGN_ID "" CACHE STRING "codesign identity; pass - for ad-hoc")
set(DRIVER_NAME "HUSH")
set(DRIVER_VERSION "0.1.0")
set(DRIVER_COPYRIGHT "Copyright (c) HUSH contributors")
set(DRIVER_IDENTIFIER "com.hush.audio")
set(DRIVER_UID "A7C3E1B2-4F58-4D1A-9C0E-6B2F8A1D4E77")
set(DRIVER_ENTRYPOINT "HushMicEntryPoint")

include(FetchContent)
FetchContent_Declare(
    libASPL
    GIT_REPOSITORY https://github.com/gavv/libASPL.git
    GIT_TAG v3.1.2
    GIT_SHALLOW TRUE
)
FetchContent_GetProperties(libASPL)
if(NOT libaspl_POPULATED)
    FetchContent_Populate(libASPL)
endif()

include(ExternalProject)
ExternalProject_Add(libASPL_build
    SOURCE_DIR ${libaspl_SOURCE_DIR}
    BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/libASPL-build
    INSTALL_DIR ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix
    CMAKE_ARGS
        -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>
        -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE}
        -DCMAKE_OSX_ARCHITECTURES=${CMAKE_OSX_ARCHITECTURES}
        -DCMAKE_OSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}
        -DCMAKE_OSX_SYSROOT=${CMAKE_OSX_SYSROOT}
    BUILD_ALWAYS OFF
)

add_library(${DRIVER_NAME} MODULE
    Driver.cpp
    ${CMAKE_SOURCE_DIR}/../../native/ring/ring.c
)

add_dependencies(${DRIVER_NAME} libASPL_build)
target_include_directories(${DRIVER_NAME} PRIVATE
    ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix/include
    ${CMAKE_SOURCE_DIR}/../../native/ring
)
target_link_libraries(${DRIVER_NAME} PRIVATE
    ${CMAKE_CURRENT_BINARY_DIR}/libASPL-prefix/lib/libASPL.a
)

find_library(CORE_FOUNDATION CoreFoundation REQUIRED)
find_library(CORE_AUDIO CoreAudio REQUIRED)
target_link_libraries(${DRIVER_NAME} PRIVATE ${CORE_FOUNDATION} ${CORE_AUDIO})

set_target_properties(${DRIVER_NAME} PROPERTIES
    OUTPUT_NAME "${DRIVER_NAME}"
    BUNDLE TRUE
    BUNDLE_EXTENSION "driver"
    PREFIX ""
    SUFFIX ""
    MACOSX_BUNDLE ON
    MACOSX_BUNDLE_INFO_PLIST "${CMAKE_CURRENT_SOURCE_DIR}/Info.plist.in"
    MACOSX_BUNDLE_BUNDLE_NAME "${DRIVER_NAME}"
    MACOSX_BUNDLE_BUNDLE_VERSION "${DRIVER_VERSION}"
    MACOSX_BUNDLE_COPYRIGHT "${DRIVER_COPYRIGHT}"
    MACOSX_BUNDLE_GUI_IDENTIFIER "${DRIVER_IDENTIFIER}"
    MACOSX_BUNDLE_SHORT_VERSION_STRING "${DRIVER_VERSION}"
)

if(NOT CODESIGN_ID STREQUAL "")
    add_custom_command(
        TARGET ${DRIVER_NAME}
        POST_BUILD
        COMMAND codesign --force --sign "${CODESIGN_ID}"
            "${CMAKE_CURRENT_BINARY_DIR}/${DRIVER_NAME}.driver"
        COMMENT "Signing ${DRIVER_NAME}.driver"
        VERBATIM
    )
endif()
