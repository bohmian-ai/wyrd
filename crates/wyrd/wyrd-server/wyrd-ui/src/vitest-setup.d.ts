// Makes the @testing-library/jest-dom matchers (toHaveAttribute, toBeInTheDocument, …)
// visible to svelte-check by surfacing the vitest Assertion augmentation to the type program.
import '@testing-library/jest-dom/vitest';
