// Lets the unit tests import the app's TypeScript as the bundler does:
// `./Diff` finds `./Diff.ts`.
import { register } from "node:module";

register(
  "data:text/javascript," +
    encodeURIComponent(`
export async function resolve(specifier, context, next) {
  try {
    return await next(specifier, context);
  } catch (error) {
    if (specifier.startsWith(".") && !/\\.[a-z]+$/.test(specifier)) return next(specifier + ".ts", context);
    throw error;
  }
}`),
);
