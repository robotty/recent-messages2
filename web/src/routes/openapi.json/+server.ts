import { json } from "@sveltejs/kit";
import { getOpenApiSpec } from "#lib/openapi";

export const prerender = true;

export function GET() {
  return json(getOpenApiSpec());
}
