import { ChangeEvent, DragEvent, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type MrzData = {
  documentNumber: string;
  surname: string;
  givenNames: string;
  nationality: string;
  birthDate: string;
  sex: string;
  expiryDate: string;
};

type ExtractedDocument = {
  rawOcr: string;
  mrz?: MrzData;
  fields: Record<string, string>;
};

const FIELD_LABELS: Record<string, string> = {
  documentNumber: "Numero de documento",
  surname: "Apellidos",
  givenNames: "Nombre",
  nationality: "Nacionalidad",
  birthDate: "Fecha de nacimiento",
  sex: "Sexo",
  expiryDate: "Fecha de caducidad",
  issuingCountry: "Pais emisor",
  documentType: "Tipo de documento",
  fullName: "Nombre completo",
};

function fileToDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(new Error("No se pudo leer el archivo."));
    reader.readAsDataURL(file);
  });
}

function normalizeFields(result?: ExtractedDocument) {
  if (!result) {
    return {};
  }

  return {
    ...result.fields,
    ...(result.mrz ?? {}),
  };
}

function App() {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [selectedName, setSelectedName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [result, setResult] = useState<ExtractedDocument | null>(null);
  const [editableFields, setEditableFields] = useState<Record<string, string>>({});

  const orderedFields = useMemo(() => {
    const preferredOrder = [
      "documentType",
      "documentNumber",
      "surname",
      "givenNames",
      "fullName",
      "nationality",
      "issuingCountry",
      "birthDate",
      "sex",
      "expiryDate",
    ];

    const known = preferredOrder.filter((key) => key in editableFields);
    const custom = Object.keys(editableFields)
      .filter((key) => !preferredOrder.includes(key))
      .sort((a, b) => a.localeCompare(b));

    return [...known, ...custom];
  }, [editableFields]);

  function applyResult(nextResult: ExtractedDocument) {
    setResult(nextResult);
    setEditableFields(normalizeFields(nextResult));
  }

  function onFileSelected(file: File | null) {
    setSelectedFile(file);
    setSelectedName(file?.name ?? "");
    setError("");
  }

  function onInputFile(event: ChangeEvent<HTMLInputElement>) {
    onFileSelected(event.currentTarget.files?.[0] ?? null);
  }

  function onDrop(event: DragEvent<HTMLLabelElement>) {
    event.preventDefault();
    onFileSelected(event.dataTransfer.files?.[0] ?? null);
  }

  function onDragOver(event: DragEvent<HTMLLabelElement>) {
    event.preventDefault();
  }

  async function extractDocument() {
    if (!selectedFile) {
      setError("Selecciona una imagen de pasaporte o DNI antes de extraer.");
      return;
    }

    setLoading(true);
    setError("");

    try {
      const imageBase64 = await fileToDataUrl(selectedFile);
      const extraction = await invoke<ExtractedDocument>("extract_document", {
        imageBase64,
      });
      applyResult(extraction);
    } catch (invokeError) {
      const message =
        invokeError instanceof Error
          ? invokeError.message
          : "No se pudo extraer el documento.";
      setError(message);
    } finally {
      setLoading(false);
    }
  }

  return (
    <main className="app-shell">
      <section className="hero-card">
        <div className="hero-copy">
          <p className="eyebrow">Leon</p>
          <h1>Reconocimiento de documentos</h1>
          <p className="hero-text">
            Carga una imagen de pasaporte o documento de identidad para ejecutar
            OCR, detectar MRZ y editar los campos extraidos.
          </p>
        </div>

        <div className="upload-panel">
          <label
            className="dropzone"
            onDrop={onDrop}
            onDragOver={onDragOver}
          >
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp,image/bmp"
              onChange={onInputFile}
            />
            <span className="dropzone-title">
              Arrastra una imagen aqui o selecciona un archivo
            </span>
            <span className="dropzone-subtitle">
              Formatos recomendados: PNG, JPG, WEBP o BMP.
            </span>
            {selectedName ? (
              <span className="selected-file">{selectedName}</span>
            ) : null}
          </label>

          <button
            className="extract-button"
            onClick={extractDocument}
            disabled={!selectedFile || loading}
            type="button"
          >
            {loading ? "Extrayendo..." : "Extraer"}
          </button>
        </div>
      </section>

      {error ? <p className="error-banner">{error}</p> : null}

      <section className="results-grid">
        <article className="panel">
          <div className="panel-heading">
            <h2>Campos editables</h2>
            <p>Corrige o completa la informacion detectada.</p>
          </div>

          {orderedFields.length === 0 ? (
            <p className="empty-state">
              Los campos apareceran aqui despues de ejecutar la extraccion.
            </p>
          ) : (
            <div className="fields-grid">
              {orderedFields.map((fieldKey) => (
                <label className="field" key={fieldKey}>
                  <span>{FIELD_LABELS[fieldKey] ?? fieldKey}</span>
                  <input
                    type="text"
                    value={editableFields[fieldKey] ?? ""}
                    onChange={(event) =>
                      setEditableFields((current) => ({
                        ...current,
                        [fieldKey]: event.currentTarget.value,
                      }))
                    }
                  />
                </label>
              ))}
            </div>
          )}
        </article>

        <article className="panel">
          <div className="panel-heading">
            <h2>JSON devuelto</h2>
            <p>Respuesta completa del comando Tauri.</p>
          </div>
          <pre className="json-output">
            {result ? JSON.stringify(result, null, 2) : "Sin resultados todavia."}
          </pre>
        </article>

        <article className="panel">
          <div className="panel-heading">
            <h2>Texto OCR</h2>
            <p>Salida en bruto para depurar el reconocimiento.</p>
          </div>
          <pre className="ocr-output">{result?.rawOcr ?? "Sin OCR todavia."}</pre>
        </article>
      </section>
    </main>
  );
}

export default App;
