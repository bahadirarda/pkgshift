import type {
  Diagnostic,
  PlannedFileMutation,
} from "../domain/models.ts";

export interface TransformationResult {
  manifestMutations: PlannedFileMutation[];
  configurationMutations: PlannedFileMutation[];
  integrationMutations: PlannedFileMutation[];
  cleanupMutations: PlannedFileMutation[];
  diagnostics: Diagnostic[];
}
