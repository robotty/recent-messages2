declare module "swagger-ui-dist/swagger-ui-es-bundle.js" {
  export interface SwaggerUIOptions {
    domNode?: HTMLElement;
    dom_id?: string;
    spec?: Record<string, unknown>;
    url?: string;
    deepLinking?: boolean;
    docExpansion?: "list" | "full" | "none";
    defaultModelsExpandDepth?: number;
  }

  export default function SwaggerUIBundle(options: SwaggerUIOptions): unknown;
}

declare module "swagger-ui-dist/swagger-ui.css" {}
