# Leon

Leon es una aplicacion de escritorio construida con Tauri 2, React y TypeScript para reconocer pasaportes y documentos de identidad por OCR, extraer campos clave y detectar MRZ de pasaportes TD3.

## Stack

- Frontend: React + TypeScript + Vite
- Backend: Rust + comandos Tauri
- OCR: binario `tesseract` invocado desde Rust con `std::process`
- Parsing MRZ: parser propio para pasaportes TD3 (2 lineas de 44 caracteres, ICAO 9303)

## Requisitos previos

1. Node.js 20 o superior
2. Rust estable
3. Dependencias de Tauri 2 para escritorio segun tu sistema operativo
4. Tesseract OCR instalado en el sistema

## Instalar Tesseract

### Windows

1. Instala Tesseract OCR con un instalador compatible para Windows.
2. Asegurate de que `tesseract.exe` quede disponible en el `PATH`.
3. Si tu instalacion requiere DLL adicionales, sigue la documentacion del distribuidor y verifica que el binario responda desde terminal con:

```powershell
tesseract --version
```

### macOS

```bash
brew install tesseract
```

### Linux Debian/Ubuntu

```bash
sudo apt update
sudo apt install tesseract-ocr
```

## Crear el proyecto

Este proyecto base se genero con el comando oficial solicitado:

```bash
npm create tauri-app@latest
```

Configuracion usada:

- Nombre del proyecto: `leon`
- Nombre de la aplicacion: `leon`
- Frontend: `React + TypeScript`
- Package manager: `npm`
- Tauri: `v2`

## Instalar dependencias

```bash
npm install
```

## Ejecutar en desarrollo

```bash
npm run tauri dev
```

La ventana de la aplicacion se muestra como `Leon`.

## Funcionamiento

1. Selecciona o arrastra una imagen de pasaporte o DNI en la pantalla principal.
2. Pulsa `Extraer`.
3. El frontend envia la imagen al backend Tauri en base64.
4. Rust guarda temporalmente la imagen, ejecuta Tesseract OCR y devuelve el texto detectado.
5. Si el OCR contiene una MRZ TD3 valida o aproximada, Leon intenta extraer:
   - `documentNumber`
   - `surname`
   - `givenNames`
   - `nationality`
   - `birthDate`
   - `sex`
   - `expiryDate`
6. La interfaz muestra:
   - Campos editables
   - JSON de respuesta
   - Texto OCR bruto

## Estructura principal

- `src/App.tsx`: UI principal de Leon
- `src/App.css`: estilos de la interfaz
- `src-tauri/src/commands.rs`: comando Tauri `extract_document`
- `src-tauri/src/ocr.rs`: OCR sobre imagen base64 usando `tesseract`
- `src-tauri/src/mrz.rs`: parser MRZ TD3
- `src-tauri/src/lib.rs`: registro del comando para el frontend

## Formato de respuesta del backend

```json
{
  "rawOcr": "texto detectado por OCR",
  "mrz": {
    "documentNumber": "123456789",
    "surname": "DOE",
    "givenNames": "JOHN",
    "nationality": "ESP",
    "birthDate": "1990-01-01",
    "sex": "M",
    "expiryDate": "2030-01-01"
  },
  "fields": {
    "documentNumber": "123456789",
    "surname": "DOE",
    "givenNames": "JOHN"
  }
}
```

## Notas

- El OCR depende de la calidad de la imagen, orientacion, contraste y resolucion.
- En esta implementacion se usa el binario del sistema `tesseract` en lugar de bindings Rust directos para evitar problemas de compatibilidad entre plataformas.
- El parser MRZ esta orientado a pasaportes TD3. Otros formatos de MRZ pueden requerir ampliar `mrz.rs`.
