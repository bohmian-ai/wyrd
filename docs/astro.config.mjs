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
      sidebar: [
        {
          label: "Start here",
          collapsed: true,
          items: [
            { label: "Overview", link: "/start-here/" },
            { label: "What is Wyrd?", link: "/start-here/what-is-wyrd/" },
            { label: "Local development", link: "/start-here/local-dev/" },
            { label: "For agents", link: "/start-here/agents/" },
          ],
        },
        {
          label: "Concepts",
          collapsed: true,
          items: [
            { label: "Overview", link: "/concepts/" },
            { label: "Cards", link: "/concepts/card/" },
            { label: "Specs", link: "/concepts/spec/" },
            { label: "Runs", link: "/concepts/run/" },
            { label: "Observations", link: "/concepts/observation/" },
            { label: "Services", link: "/concepts/service-card/" },
            { label: "Policies", link: "/concepts/policy-card/" },
            { label: "Lineage", link: "/concepts/lineage/" },
            { label: "Audit", link: "/concepts/audit/" },
          ],
        },
        {
          label: "Cards",
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
          label: "Guides",
          collapsed: true,
          items: [
            { label: "Overview", link: "/guides/" },
            { label: "Register a card", link: "/guides/register-card/" },
            { label: "Lock and install a service", link: "/guides/lock-install-service/" },
          ],
        },
        {
          label: "Agents",
          collapsed: true,
          items: [
            { label: "Overview", link: "/agents/" },
            { label: "MCP usage", link: "/agents/mcp/" },
            { label: "Error remediation", link: "/agents/error-remediation/" },
          ],
        },
        {
          label: "API",
          collapsed: true,
          items: [
            { label: "Overview", link: "/api/" },
            { label: "OpenAPI", link: "/api/openapi/" },
            { label: "Errors", link: "/api/errors/" },
            { label: "Schemas", link: "/api/schemas/" },
          ],
        },
        {
          label: "CLI and MCP",
          collapsed: true,
          items: [{ label: "Overview", link: "/cli-mcp/" }],
        },
        {
          label: "Python",
          collapsed: true,
          items: [{ label: "Overview", link: "/python/" }],
        },
        {
          label: "Observability",
          collapsed: true,
          items: [{ label: "Overview", link: "/observability/" }],
        },
        {
          label: "Policy and audit",
          collapsed: true,
          items: [{ label: "Overview", link: "/policy-audit/" }],
        },
        {
          label: "Deploy",
          collapsed: true,
          items: [{ label: "Overview", link: "/deploy/" }],
        },
        {
          label: "Reference",
          collapsed: true,
          items: [
            { label: "Overview", link: "/reference/" },
            { label: "Generated docs", link: "/reference/generated-docs/" },
          ],
        },
        {
          label: "Migration",
          collapsed: true,
          items: [
            { label: "Overview", link: "/migration/" },
            { label: "Predecessor mapping", link: "/migration/predecessor-mapping/" },
          ],
        },
      ],
    }),
  ],
});
