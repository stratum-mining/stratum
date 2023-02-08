# Expectations for Implementing Prop Tests with Shrink in Arbitrary in Rust

Prop tests in Rust are used to verify the behavior of a program by generating random inputs and checking the output. The implementation of shrink in arbitrary is a key part of these tests. The following expectations should be met to ensure effective and consistent prop tests:

## Fast and High Coverage

- Run quickly and generate a large number of test cases for coverage

## Repeatable and Deterministic

- Be repeatable and deterministic

## Comprehensive

- Cover a wide range of inputs and edge cases

## Use Shrink in Arbitrary

- Use shrink in arbitrary to find the smallest input that produces an incorrect result

## Readable and Maintainable

- Be readable and maintainable with clear documentation

## Follow Established Shrink Pattern

- Follow the established shrink pattern in arbitrary that has been implemented

In summary, prop tests with shrink in arbitrary should meet these criteria to ensure their effectiveness and consistency in verifying the behavior of the program.
