cmake_minimum_required(VERSION 3.28...3.30)

project(cli-capture)

add_executable(cli-capture
  main.cpp
)

target_link_libraries(cli-capture
  PRIVATE
    libobs
)

set_target_properties(cli-capture PROPERTIES
  FOLDER "cli-capture"
  PREFIX ""
  OUTPUT_NAME "cli-capture"
)

if(MSVC)
  target_compile_options(cli-capture PRIVATE /W3 /wd4996 /wd4099 /utf-8)
endif()
