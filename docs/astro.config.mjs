import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  integrations: [
    starlight({
      title: "Wyrd",
      description: "Developer and agent documentation for Wyrd.",
      logo: {
        src: "./src/assets/wyrd-mark.svg",
        alt: "Wyrd",
      },
      head: [
        {
          tag: "link",
          attrs: {
            rel: "icon",
            href: "/favicon.svg",
            type: "image/svg+xml",
          },
        },
      ],
      customCss: ["./src/styles/wyrd.css"],
      expressiveCode: {
        themes: ["github-light", "github-dark"],
        defaultProps: { frame: "none" },
        styleOverrides: {
          borderWidth: "1px",
          borderColor: "var(--sl-color-hairline)",
          codeFontSize: "0.82rem",
        },
      },
      sidebar: [
        { label: "Overview", link: "/" },
        {
          label: "Get started",
          collapsed: true,
          items: [
            { label: "What is Wyrd?", link: "/start-here/what-is-wyrd/" },
            { label: "Quickstart", link: "/start-here/quickstart/" },
            { label: "Local development", link: "/start-here/local-dev/" },
            { label: "For agents", link: "/start-here/agents/" },
          ],
        },
        {
          label: "Concepts",
          collapsed: true,
          items: [
            { label: "Overview", link: "/concepts/" },
            { label: "Core doctrine", link: "/concepts/core-doctrine/" },
            { label: "Cards", link: "/concepts/card/" },
            { label: "Specs", link: "/concepts/spec/" },
            { label: "Runs", link: "/concepts/run/" },
            { label: "Observations", link: "/concepts/observation/" },
            { label: "Policies", link: "/concepts/policy-card/" },
            { label: "Audit", link: "/concepts/audit/" },
            { label: "Lineage", link: "/concepts/lineage/" },
            { label: "Services", link: "/concepts/service-card/" },
          ],
        },
        {
          label: "Card reference",
          collapsed: true,
          items: [
            { label: "Overview", link: "/cards/" },
            { label: "Agent", link: "/cards/agent/" },
            { label: "Artifact", link: "/cards/artifact/" },
            { label: "Audit", link: "/cards/audit/" },
            { label: "Data", link: "/cards/data/" },
            { label: "Drift", link: "/cards/drift/" },
            { label: "Eval", link: "/cards/eval/" },
            { label: "Experiment", link: "/cards/experiment/" },
            { label: "MCP", link: "/cards/mcp/" },
            { label: "Model", link: "/cards/model/" },
            { label: "Operator", link: "/cards/operator/" },
            { label: "Policy", link: "/cards/policy/" },
            { label: "Prompt", link: "/cards/prompt/" },
            { label: "Service", link: "/cards/service/" },
            { label: "Skill", link: "/cards/skill/" },
            { label: "SubAgent", link: "/cards/subagent/" },
            { label: "Tool", link: "/cards/tool/" },
            { label: "Trigger", link: "/cards/trigger/" },
            { label: "Workflow", link: "/cards/workflow/" },
          ],
        },
        {
          label: "Features",
          collapsed: true,
          items: [
            {
              label: "Essential",
              collapsed: true,
              items: [
                { label: "Register a card", link: "/guides/register-card/" },
                { label: "Lock and install a service", link: "/guides/lock-install-service/" },
                { label: "Policy and audit", link: "/policy-audit/" },
                { label: "Observability", link: "/observability/" },
              ],
            },
            {
              label: "Advanced",
              collapsed: true,
              items: [
                { label: "Deploy", link: "/deploy/" },
                { label: "Migration", link: "/migration/" },
              ],
            },
          ],
        },
        {
          label: "Integrations",
          collapsed: true,
          items: [
            { label: "Agents and MCP", link: "/agents/" },
            { label: "MCP usage", link: "/agents/mcp/" },
            { label: "Error remediation", link: "/agents/error-remediation/" },
            { label: "CLI and MCP", link: "/cli-mcp/" },
            { label: "Python SDK", link: "/python/" },
          ],
        },
        {
          label: "API reference",
          collapsed: true,
          items: [
            { label: "Overview", link: "/api/" },
            { label: "OpenAPI", link: "/api/openapi/" },
            { label: "Errors", link: "/api/errors/" },
            { label: "Schemas", link: "/api/schemas/" },
          ],
        },
        {
          label: "Reference",
          collapsed: true,
          items: [
            { label: "Overview", link: "/reference/" },
            { label: "Generated docs", link: "/reference/generated-docs/" },
            { label: "Predecessor mapping", link: "/migration/predecessor-mapping/" },
          ],
        },
      ],
    }),
  ],
});
