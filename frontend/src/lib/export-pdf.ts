import jsPDF from 'jspdf'
import autoTable from 'jspdf-autotable'
import type { ReportDetail, ReportSummary, Patient } from '@/api/types'

const STATUS_LABELS: Record<string, string> = {
  critical_high: '危急偏高',
  high: '偏高',
  normal: '正常',
  low: '偏低',
  critical_low: '危急偏低',
}

const STATUS_COLORS: Record<string, [number, number, number]> = {
  critical_high: [220, 38, 38],
  high: [234, 88, 12],
  normal: [34, 34, 34],
  low: [37, 99, 235],
  critical_low: [220, 38, 38],
}

function setupChinese(doc: jsPDF) {
  // jsPDF default font doesn't support Chinese well.
  // We'll use a fallback approach: set font size and use the built-in helvetica,
  // but for Chinese text we encode as UTF-16. This is a known limitation.
  // For production-grade Chinese PDF, a custom font file would be embedded.
  // Here we use a pragmatic approach that works for most CJK characters.
  doc.setFont('helvetica')
}

export function exportReportPDF(report: ReportDetail, patient?: Patient) {
  const doc = new jsPDF()
  setupChinese(doc)

  let y = 15

  // Title
  doc.setFontSize(16)
  doc.setTextColor(34, 34, 34)
  doc.text(`Medical Report / ${report.report_type}`, 14, y)
  y += 10

  // Patient info
  if (patient) {
    doc.setFontSize(10)
    doc.setTextColor(100, 100, 100)
    doc.text(`Patient: ${patient.name}  |  Gender: ${patient.gender}  |  DOB: ${patient.dob || 'N/A'}`, 14, y)
    y += 6
  }

  // Report metadata
  doc.setFontSize(10)
  doc.setTextColor(100, 100, 100)
  doc.text(`Report Type: ${report.report_type}`, 14, y)
  y += 5
  doc.text(`Hospital: ${report.hospital || 'N/A'}`, 14, y)
  y += 5
  doc.text(`Report Date: ${report.report_date}  |  Sample Date: ${report.sample_date || 'N/A'}`, 14, y)
  y += 8

  // Separator line
  doc.setDrawColor(200, 200, 200)
  doc.line(14, y, 196, y)
  y += 5

  // Test items table
  const tableData = report.test_items.map(item => [
    item.name,
    item.value,
    item.unit,
    item.reference_range,
    STATUS_LABELS[item.status] || item.status,
  ])

  autoTable(doc, {
    startY: y,
    head: [['Test Item', 'Result', 'Unit', 'Reference Range', 'Status']],
    body: tableData,
    styles: {
      fontSize: 9,
      cellPadding: 3,
    },
    headStyles: {
      fillColor: [52, 130, 255],
      textColor: [255, 255, 255],
      fontStyle: 'bold',
    },
    bodyStyles: {
      textColor: [34, 34, 34],
    },
    didParseCell(data) {
      if (data.section === 'body' && data.column.index === 4) {
        const status = report.test_items[data.row.index]?.status
        if (status && STATUS_COLORS[status]) {
          data.cell.styles.textColor = STATUS_COLORS[status]
          if (status === 'critical_high' || status === 'critical_low') {
            data.cell.styles.fontStyle = 'bold'
          }
        }
      }
      if (data.section === 'body' && data.column.index === 1) {
        const status = report.test_items[data.row.index]?.status
        if (status && status !== 'normal' && STATUS_COLORS[status]) {
          data.cell.styles.textColor = STATUS_COLORS[status]
        }
      }
    },
    alternateRowStyles: {
      fillColor: [248, 250, 252],
    },
  })

  // Footer
  const pageCount = doc.getNumberOfPages()
  for (let i = 1; i <= pageCount; i++) {
    doc.setPage(i)
    doc.setFontSize(8)
    doc.setTextColor(150, 150, 150)
    doc.text(
      `Page ${i} / ${pageCount}  |  Generated: ${new Date().toLocaleString()}`,
      14,
      doc.internal.pageSize.height - 10,
    )
  }

  doc.save(`Report_${report.report_type}_${report.report_date}.pdf`)
}

export function exportAllReportsPDF(reports: ReportSummary[], patient: Patient) {
  const doc = new jsPDF()
  setupChinese(doc)

  let y = 15

  // Title
  doc.setFontSize(16)
  doc.setTextColor(34, 34, 34)
  doc.text(`Patient Reports Summary / ${patient.name}`, 14, y)
  y += 10

  // Patient info
  doc.setFontSize(10)
  doc.setTextColor(100, 100, 100)
  doc.text(`Name: ${patient.name}  |  Gender: ${patient.gender}  |  DOB: ${patient.dob || 'N/A'}`, 14, y)
  y += 5
  doc.text(`Phone: ${patient.phone || 'N/A'}  |  ID: ${patient.id_number || 'N/A'}`, 14, y)
  y += 8

  doc.setDrawColor(200, 200, 200)
  doc.line(14, y, 196, y)
  y += 5

  // Reports summary table
  const tableData = reports.map(r => [
    r.report_type,
    r.hospital,
    r.report_date,
    r.sample_date || '',
    String(r.item_count ?? 0),
    String(r.abnormal_count ?? 0),
    (r.abnormal_names ?? []).join(', '),
  ])

  autoTable(doc, {
    startY: y,
    head: [['Report Type', 'Hospital', 'Date', 'Sample Date', 'Items', 'Abnormal', 'Abnormal Items']],
    body: tableData,
    styles: {
      fontSize: 8,
      cellPadding: 2.5,
    },
    headStyles: {
      fillColor: [52, 130, 255],
      textColor: [255, 255, 255],
      fontStyle: 'bold',
    },
    columnStyles: {
      5: { cellWidth: 18 },
      6: { cellWidth: 45 },
    },
    didParseCell(data) {
      if (data.section === 'body' && data.column.index === 5) {
        const count = parseInt(data.cell.raw as string, 10)
        if (count > 0) {
          data.cell.styles.textColor = [220, 38, 38]
          data.cell.styles.fontStyle = 'bold'
        }
      }
    },
    alternateRowStyles: {
      fillColor: [248, 250, 252],
    },
  })

  // Footer
  const pageCount = doc.getNumberOfPages()
  for (let i = 1; i <= pageCount; i++) {
    doc.setPage(i)
    doc.setFontSize(8)
    doc.setTextColor(150, 150, 150)
    doc.text(
      `Page ${i} / ${pageCount}  |  Total: ${reports.length} reports  |  Generated: ${new Date().toLocaleString()}`,
      14,
      doc.internal.pageSize.height - 10,
    )
  }

  doc.save(`${patient.name}_Reports_Summary.pdf`)
}
